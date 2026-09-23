use std::path::Path;

use cutix_playback::AudioBuffer;
use video::ffmpeg::H264Encoder;

use crate::backend::{
    AudioSpec, AudioSupport, BackendFactory, EncoderBackend, ExportArtifacts, VideoSpec,
};
use crate::error::{ExportError, Result};
use crate::mp4_sink::Mp4Sink;

pub struct FfmpegMp4Factory;

pub static FFMPEG_MP4: FfmpegMp4Factory = FfmpegMp4Factory;

pub const NAME: &str = "ffmpeg-h264-mp4";

pub fn chosen_encoder() -> Option<&'static str> {
    video::ffmpeg::best_h264_encoder()
}

pub fn is_hardware() -> bool {
    chosen_encoder().is_some_and(video::ffmpeg::is_hardware_h264)
}

impl BackendFactory for FfmpegMp4Factory {
    fn name(&self) -> &'static str {
        NAME
    }

    fn is_available(&self) -> bool {
        chosen_encoder().is_some()
    }

    fn is_hardware(&self) -> bool {
        is_hardware()
    }

    fn audio_support(&self) -> AudioSupport {
        crate::mp4_sink::audio_support()
    }

    fn create(
        &self,
        destination: &Path,
        video: VideoSpec,
        _audio: Option<AudioSpec>,
    ) -> Result<Box<dyn EncoderBackend>> {
        Ok(Box::new(FfmpegMp4Backend::new(destination, video)?))
    }
}

struct FfmpegMp4Backend {
    sink: Mp4Sink,
    spec: VideoSpec,
    encoder: H264Encoder,
    flushed: bool,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
}

impl FfmpegMp4Backend {
    fn new(destination: &Path, spec: VideoSpec) -> Result<Self> {
        let name = chosen_encoder()
            .ok_or_else(|| ExportError::Encoder(video::ffmpeg::status().to_string()))?;
        if !spec.frame_rate.is_valid() {
            return Err(ExportError::InvalidFrameRate);
        }

        let sink = Mp4Sink::create(destination, spec)?;
        let encoder = H264Encoder::open(
            name,
            spec.width,
            spec.height,
            (spec.frame_rate.numerator, spec.frame_rate.denominator),
            spec.bitrate_bps,
        )
        .map_err(ExportError::Encoder)?;

        let luma = spec.width as usize * spec.height as usize;
        Ok(Self {
            sink,
            spec,
            encoder,
            flushed: false,
            y: vec![0u8; luma],
            u: vec![0u8; luma / 4],
            v: vec![0u8; luma / 4],
        })
    }
}

impl EncoderBackend for FfmpegMp4Backend {
    fn name(&self) -> &'static str {
        NAME
    }

    fn audio_support(&self) -> AudioSupport {
        crate::mp4_sink::audio_support()
    }

    fn push_frame(&mut self, rgba: &[u8]) -> Result<()> {
        if self.flushed {
            return Err(ExportError::Encoder(
                "a frame arrived after the encoder was drained for audio".to_string(),
            ));
        }
        let width = self.spec.width as usize;
        let height = self.spec.height as usize;
        let expected = width * height * 4;
        if rgba.len() < expected {
            return Err(ExportError::Encoder(format!(
                "frame is {} bytes, expected {expected}",
                rgba.len()
            )));
        }

        video::color::rgba_to_i420(
            &rgba[..expected],
            (width, height),
            &mut self.y,
            &mut self.u,
            &mut self.v,
        );
        let packets = self
            .encoder
            .encode(&self.y, &self.u, &self.v)
            .map_err(ExportError::Encoder)?;
        for packet in packets {
            self.sink.push_annex_b(&packet.bytes, packet.is_sync)?;
        }
        Ok(())
    }

    fn push_audio(&mut self, audio: &AudioBuffer) -> Result<()> {
        if !audio.interleaved.is_empty() {
            self.flush_encoder()?;
        }
        self.sink.push_audio(audio)
    }

    fn finish(mut self: Box<Self>) -> Result<ExportArtifacts> {
        self.flush_encoder()?;
        self.sink.finish()
    }
}

impl FfmpegMp4Backend {
    fn flush_encoder(&mut self) -> Result<()> {
        if self.flushed {
            return Ok(());
        }
        self.flushed = true;
        let packets = self.encoder.finish().map_err(ExportError::Encoder)?;
        for packet in packets {
            self.sink.push_annex_b(&packet.bytes, packet.is_sync)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_factory_name_is_distinct_from_the_shipped_one() {
        assert_ne!(NAME, crate::openh264_mp4::OPENH264_MP4.name());
    }

    #[test]
    fn an_export_shorter_than_the_encoder_lookahead_still_takes_its_audio() {
        if !FFMPEG_MP4.is_available() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("short.mp4");
        let (width, height) = (320u32, 240u32);
        let spec = VideoSpec {
            width,
            height,
            frame_rate: time::FrameRate::FPS_30,
            bitrate_bps: 1_000_000,
            quality: crate::presets::ExportQuality::Medium,
        };
        let mut backend = FfmpegMp4Backend::new(&path, spec).expect("backend");
        let frame = vec![128u8; width as usize * height as usize * 4];
        for _ in 0..5 {
            backend.push_frame(&frame).expect("frame");
        }
        if backend.sink.frames() > 0 {
            eprintln!(
                "SKIPPED: {} emitted packets before the audio",
                chosen_encoder().unwrap_or("the encoder")
            );
            return;
        }
        let audio = AudioBuffer {
            sample_rate: 48_000,
            channels: 2,
            interleaved: (0..48_000 / 6)
                .flat_map(|index| {
                    let value = (index as f32 * 0.05).sin() * 0.3;
                    [value, value]
                })
                .collect(),
        };
        backend.push_audio(&audio).expect("audio after five frames");
        let artifacts = Box::new(backend).finish().expect("finish");
        assert_eq!(artifacts.frames, 5);
        assert!(artifacts.audio_samples > 0);
    }

    #[test]
    fn availability_and_hardware_agree_with_the_probe() {
        assert_eq!(FFMPEG_MP4.is_available(), chosen_encoder().is_some());
        if !FFMPEG_MP4.is_available() {
            assert!(!FFMPEG_MP4.is_hardware());
        }
    }
}
