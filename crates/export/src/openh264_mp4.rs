use std::path::Path;

use cutix_playback::AudioBuffer;
use openh264::encoder::{
    Encoder, EncoderConfig, FrameRate as EncoderFrameRate, FrameType, IntraFramePeriod, Level,
    Profile, QpRange, RateControlMode, SpsPpsStrategy,
};
use openh264::formats::{RgbSliceU8, YUVBuffer};
use openh264::{OpenH264API, Timestamp};

use crate::backend::{
    AudioSpec, AudioSupport, BackendFactory, EncoderBackend, ExportArtifacts, VideoSpec,
};
use crate::error::{ExportError, Result};
use crate::mp4_sink::{Mp4Sink, TIMESCALE};

pub struct OpenH264Mp4Factory;

pub static OPENH264_MP4: OpenH264Mp4Factory = OpenH264Mp4Factory;

pub fn audio_support() -> AudioSupport {
    crate::mp4_sink::audio_support()
}

impl BackendFactory for OpenH264Mp4Factory {
    fn name(&self) -> &'static str {
        "openh264-mp4"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn is_hardware(&self) -> bool {
        false
    }

    fn audio_support(&self) -> AudioSupport {
        audio_support()
    }

    fn create(
        &self,
        destination: &Path,
        video: VideoSpec,
        _audio: Option<AudioSpec>,
    ) -> Result<Box<dyn EncoderBackend>> {
        Ok(Box::new(OpenH264Mp4Backend::new(destination, video)?))
    }
}

struct OpenH264Mp4Backend {
    sink: Mp4Sink,
    spec: VideoSpec,
    encoder: Encoder,
    yuv: YUVBuffer,
    rgb: Vec<u8>,
    sample_duration: u32,
}

impl OpenH264Mp4Backend {
    fn new(destination: &Path, spec: VideoSpec) -> Result<Self> {
        let fps = spec
            .frame_rate
            .as_f64()
            .filter(|value| *value > 0.0)
            .ok_or(ExportError::InvalidFrameRate)?;
        let sample_duration = ((TIMESCALE as f64) / fps).round() as u32;

        // OpenH264 can only hold a target bitrate by dropping frames when it overshoots,
        // and an export that silently drops frames produces a file shorter than the
        // timeline. Frame skipping therefore stays off, which means bitrate targeting is
        // not a promise this encoder can keep — declaring one makes the library print a
        // warning and ignore it. The export quality is carried as a fixed quantiser
        // instead, which quality-mode rate control really does honour.
        //
        // OpenH264 still prints "bitrate can't be controlled ... without enabling skip
        // frame" on open. That warning is accurate and is the contract we want: bitrate is
        // not controlled here. Enabling frame skip to silence it would trade a correct
        // frame count for a bitrate nobody asked this backend to hold.
        let quantiser = spec.quality.quantiser();
        let config = EncoderConfig::new()
            .max_frame_rate(EncoderFrameRate::from_hz(fps as f32))
            .rate_control_mode(RateControlMode::Quality)
            .qp(QpRange::new(quantiser, quantiser))
            .sps_pps_strategy(SpsPpsStrategy::ConstantId)
            .profile(Profile::High)
            .level(Level::Level_5_1)
            .intra_frame_period(IntraFramePeriod::from_num_frames(
                (fps.round() as u32).clamp(1, 300),
            ))
            .skip_frames(false);

        let sink = Mp4Sink::create(destination, spec)?;
        let encoder = Encoder::with_api_config(OpenH264API::from_source(), config)
            .map_err(|error| ExportError::Encoder(error.to_string()))?;

        Ok(Self {
            sink,
            spec,
            encoder,
            yuv: YUVBuffer::new(spec.width as usize, spec.height as usize),
            rgb: vec![0u8; spec.width as usize * spec.height as usize * 3],
            sample_duration,
        })
    }
}

impl EncoderBackend for OpenH264Mp4Backend {
    fn name(&self) -> &'static str {
        "openh264-mp4"
    }

    fn audio_support(&self) -> AudioSupport {
        audio_support()
    }

    fn push_frame(&mut self, rgba: &[u8]) -> Result<()> {
        let expected = self.spec.width as usize * self.spec.height as usize * 4;
        if rgba.len() < expected {
            return Err(ExportError::Encoder(format!(
                "frame is {} bytes, expected {expected}",
                rgba.len()
            )));
        }

        for (target, source) in self
            .rgb
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .zip(rgba.as_chunks::<4>().0)
        {
            target.copy_from_slice(&source[..3]);
        }
        self.yuv.read_rgb8(RgbSliceU8::new(
            &self.rgb,
            (self.spec.width as usize, self.spec.height as usize),
        ));

        let index = self.sink.frames();
        let timestamp =
            Timestamp::from_millis(index * self.sample_duration as u64 * 1000 / TIMESCALE as u64);

        let stream = self
            .encoder
            .encode_at(&self.yuv, timestamp)
            .map_err(|error| ExportError::Encoder(error.to_string()))?;
        let frame_type = stream.frame_type();
        if matches!(frame_type, FrameType::Skip | FrameType::Invalid) {
            return Err(ExportError::Encoder(format!(
                "encoder produced no data for frame {index} ({frame_type:?})"
            )));
        }
        self.sink
            .push_annex_b(&stream.to_vec(), matches!(frame_type, FrameType::IDR))
    }

    fn push_audio(&mut self, audio: &AudioBuffer) -> Result<()> {
        self.sink.push_audio(audio)
    }

    fn finish(self: Box<Self>) -> Result<ExportArtifacts> {
        self.sink.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_odd_output_size_is_rejected_before_any_file_is_created() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("odd.mp4");
        let outcome = OpenH264Mp4Backend::new(
            &path,
            VideoSpec {
                width: 101,
                height: 100,
                frame_rate: time::FrameRate::FPS_30,
                bitrate_bps: 1_000_000,
                quality: crate::presets::ExportQuality::Medium,
            },
        );
        assert!(matches!(
            outcome.err(),
            Some(ExportError::InvalidSize { .. })
        ));
        assert!(!path.exists());
    }

    #[test]
    fn the_shipped_backend_is_never_a_hardware_one() {
        assert!(!OPENH264_MP4.is_hardware());
        assert!(OPENH264_MP4.is_available());
    }
}
