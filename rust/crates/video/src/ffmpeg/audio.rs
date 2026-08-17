use std::ffi::{c_int, c_void, CString};
use std::path::Path;

use super::sys::{self, AVFormatContext, AVFrame, AVPacket, AVStream};
use super::{null_mut, null_void, Api};
use crate::decode::DecodeError;

#[derive(Clone, Debug)]
pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: usize,
    pub samples: Vec<Vec<f32>>,
}

impl AudioBuffer {
    pub fn frame_count(&self) -> usize {
        self.samples.first().map(Vec::len).unwrap_or(0)
    }

    pub fn duration_seconds(&self) -> f64 {
        self.frame_count() as f64 / f64::from(self.sample_rate.max(1))
    }
}

fn backend_error(detail: impl Into<String>) -> DecodeError {
    DecodeError::Backend(detail.into())
}

struct AudioDecoder {
    api: &'static Api,
    format: *mut AVFormatContext,
    codec: *mut c_void,
    packet: *mut AVPacket,
    frame: *mut AVFrame,
    resampler: *mut c_void,
    layout: sys::AVChannelLayout,
    input_format: c_int,
    stream_index: c_int,
    sample_rate: u32,
    channels: usize,
}

impl Drop for AudioDecoder {
    fn drop(&mut self) {
        unsafe {
            if !self.resampler.is_null() {
                (self.api.swr_free)(&mut self.resampler);
            }
            if !self.frame.is_null() {
                (self.api.av_frame_free)(&mut self.frame);
            }
            if !self.packet.is_null() {
                (self.api.av_packet_free)(&mut self.packet);
            }
            if !self.codec.is_null() {
                (self.api.avcodec_free_context)(&mut self.codec);
            }
            if !self.format.is_null() {
                (self.api.avformat_close_input)(&mut self.format);
            }
        }
    }
}

impl AudioDecoder {
    fn open(path: &Path) -> Result<Self, DecodeError> {
        let ffmpeg = super::instance().map_err(backend_error)?;
        let api = ffmpeg.api();

        let as_text = path
            .to_str()
            .ok_or_else(|| DecodeError::Io("path is not valid UTF-8".into()))?;
        let c_path = CString::new(as_text)
            .map_err(|_| DecodeError::Io("path contains an interior NUL".into()))?;

        let mut format: *mut AVFormatContext = null_mut();
        let opened = unsafe {
            (api.avformat_open_input)(&mut format, c_path.as_ptr(), null_void(), null_mut())
        };
        if opened < 0 || format.is_null() {
            return Err(DecodeError::Container(format!(
                "avformat_open_input failed ({opened}) for {}",
                path.display()
            )));
        }

        let mut decoder = Self {
            api,
            format,
            codec: null_void(),
            packet: null_mut(),
            frame: null_mut(),
            resampler: null_void(),
            layout: sys::AVChannelLayout::EMPTY,
            input_format: -1,
            stream_index: -1,
            sample_rate: 0,
            channels: 0,
        };

        if unsafe { (api.avformat_find_stream_info)(decoder.format, null_mut()) } < 0 {
            return Err(DecodeError::Container("no stream info".into()));
        }

        decoder.stream_index = unsafe {
            (api.av_find_best_stream)(
                decoder.format,
                sys::AVMEDIA_TYPE_AUDIO,
                -1,
                -1,
                null_mut(),
                0,
            )
        };
        if decoder.stream_index < 0 {
            return Err(backend_error("file has no audio track"));
        }

        let count = unsafe { (*decoder.format).nb_streams } as c_int;
        if decoder.stream_index >= count {
            return Err(backend_error("audio stream index out of range"));
        }
        let stream: *mut AVStream =
            unsafe { *(*decoder.format).streams.add(decoder.stream_index as usize) };
        let codecpar = unsafe { (*stream).codecpar };
        if codecpar.is_null() {
            return Err(backend_error("audio stream has no parameters"));
        }

        decoder.sample_rate = unsafe { (*codecpar).sample_rate }.max(0) as u32;
        decoder.channels = unsafe { (*codecpar).ch_layout.nb_channels }.max(0) as usize;
        if decoder.sample_rate == 0 || decoder.channels == 0 {
            return Err(backend_error(format!(
                "unusable audio parameters: {} Hz, {} channels",
                decoder.sample_rate, decoder.channels
            )));
        }

        let codec = unsafe { (api.avcodec_find_decoder)((*codecpar).codec_id) };
        if codec.is_null() {
            return Err(backend_error(format!(
                "no decoder for audio codec id {}",
                unsafe { (*codecpar).codec_id }
            )));
        }

        decoder.codec = unsafe { (api.avcodec_alloc_context3)(codec) };
        if decoder.codec.is_null() {
            return Err(backend_error("avcodec_alloc_context3 failed"));
        }
        if unsafe { (api.avcodec_parameters_to_context)(decoder.codec, codecpar) } < 0 {
            return Err(backend_error("avcodec_parameters_to_context failed"));
        }
        if unsafe { (api.avcodec_open2)(decoder.codec, codec, null_mut()) } < 0 {
            return Err(backend_error("avcodec_open2 failed for audio"));
        }

        decoder.layout = unsafe { (*codecpar).ch_layout };

        decoder.packet = unsafe { (api.av_packet_alloc)() };
        decoder.frame = unsafe { (api.av_frame_alloc)() };
        if decoder.packet.is_null() || decoder.frame.is_null() {
            return Err(backend_error("packet or frame allocation failed"));
        }

        Ok(decoder)
    }

    fn seek(&mut self, seconds: f64) -> Result<(), DecodeError> {
        let target = (seconds.max(0.0) * super::video::AV_TIME_BASE) as i64;
        let sought = unsafe {
            (self.api.avformat_seek_file)(
                self.format,
                -1,
                i64::MIN,
                target,
                target,
                sys::AVSEEK_FLAG_BACKWARD,
            )
        };
        if sought < 0 {
            return Err(DecodeError::Container(format!(
                "avformat_seek_file failed ({sought}) seeking audio to {seconds:.3}s"
            )));
        }
        unsafe { (self.api.avcodec_flush_buffers)(self.codec) };
        Ok(())
    }

    fn drain(&mut self) -> Result<AudioBuffer, DecodeError> {
        self.drain_frames(usize::MAX)
    }

    fn drain_frames(&mut self, limit: usize) -> Result<AudioBuffer, DecodeError> {
        let mut planes: Vec<Vec<f32>> = vec![Vec::new(); self.channels];
        let mut scratch: Vec<Vec<f32>> = vec![Vec::new(); self.channels];
        let mut finished = false;

        loop {
            if planes.first().is_some_and(|plane| plane.len() >= limit) {
                break;
            }
            let received = unsafe { (self.api.avcodec_receive_frame)(self.codec, self.frame) };
            if received == 0 {
                let count = unsafe { (*self.frame).nb_samples }.max(0) as usize;
                if count > 0 {
                    self.convert(count, &mut scratch, &mut planes)?;
                }
                unsafe { (self.api.av_frame_unref)(self.frame) };
                continue;
            }
            if received == sys::AVERROR_EOF {
                break;
            }
            if received != sys::AVERROR_EAGAIN {
                return Err(DecodeError::Decoder(format!(
                    "avcodec_receive_frame failed ({received})"
                )));
            }
            if finished {
                break;
            }

            let read = unsafe { (self.api.av_read_frame)(self.format, self.packet) };
            if read < 0 {
                unsafe { (self.api.avcodec_send_packet)(self.codec, null_mut()) };
                finished = true;
                continue;
            }
            let index = unsafe { (*self.packet).stream_index };
            if index == self.stream_index {
                unsafe { (self.api.avcodec_send_packet)(self.codec, self.packet) };
            }
            unsafe { (self.api.av_packet_unref)(self.packet) };
        }

        if planes.iter().all(Vec::is_empty) {
            return Err(DecodeError::NoFrame);
        }

        Ok(AudioBuffer {
            sample_rate: self.sample_rate,
            channels: self.channels,
            samples: planes,
        })
    }

    fn prepare_resampler(&mut self) -> Result<(), DecodeError> {
        let format = unsafe { (*self.frame).format };
        if !self.resampler.is_null() && self.input_format == format {
            return Ok(());
        }
        if !self.resampler.is_null() {
            unsafe { (self.api.swr_free)(&mut self.resampler) };
            self.resampler = null_void();
        }
        self.input_format = -1;

        let mut resampler: *mut c_void = null_void();
        let created = unsafe {
            (self.api.swr_alloc_set_opts2)(
                &mut resampler,
                &self.layout,
                sys::AV_SAMPLE_FMT_FLTP,
                self.sample_rate as c_int,
                &self.layout,
                format,
                self.sample_rate as c_int,
                0,
                null_void(),
            )
        };
        if created < 0 || resampler.is_null() {
            return Err(backend_error(format!(
                "swr_alloc_set_opts2 failed for sample format {format}"
            )));
        }
        self.resampler = resampler;
        if unsafe { (self.api.swr_init)(self.resampler) } < 0 {
            return Err(backend_error(format!(
                "swr_init failed for sample format {format}"
            )));
        }
        self.input_format = format;
        Ok(())
    }

    fn convert(
        &mut self,
        count: usize,
        scratch: &mut [Vec<f32>],
        planes: &mut [Vec<f32>],
    ) -> Result<(), DecodeError> {
        for plane in scratch.iter_mut() {
            plane.clear();
            plane.resize(count, 0.0);
        }

        let mut destination: Vec<*mut u8> = scratch
            .iter_mut()
            .map(|plane| plane.as_mut_ptr().cast::<u8>())
            .collect();

        self.prepare_resampler()?;

        let source = unsafe { (*self.frame).extended_data.cast::<*const u8>() };
        let converted = unsafe {
            (self.api.swr_convert)(
                self.resampler,
                destination.as_mut_ptr(),
                count as c_int,
                source,
                count as c_int,
            )
        };
        if converted < 0 {
            return Err(DecodeError::Decoder("swr_convert failed".into()));
        }

        let produced = converted as usize;
        for (plane, source) in planes.iter_mut().zip(scratch.iter()) {
            plane.extend_from_slice(&source[..produced.min(source.len())]);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AudioInfo {
    pub sample_rate: u32,
    pub channels: usize,
    pub duration_seconds: f64,
}

pub fn probe_audio(path: impl AsRef<Path>) -> Result<AudioInfo, DecodeError> {
    let decoder = AudioDecoder::open(path.as_ref())?;
    let ticks = unsafe { (*decoder.format).duration };
    Ok(AudioInfo {
        sample_rate: decoder.sample_rate,
        channels: decoder.channels,
        duration_seconds: if ticks > 0 {
            ticks as f64 / 1_000_000.0
        } else {
            0.0
        },
    })
}

pub fn decode_audio(path: impl AsRef<Path>) -> Result<AudioBuffer, DecodeError> {
    let mut decoder = AudioDecoder::open(path.as_ref())?;
    decoder.drain()
}

pub fn decode_audio_range(
    path: impl AsRef<Path>,
    start_seconds: f64,
    seconds: f64,
) -> Result<(AudioBuffer, f64), DecodeError> {
    let mut decoder = AudioDecoder::open(path.as_ref())?;
    let start = start_seconds.max(0.0);
    decoder.seek(start)?;
    let limit = (seconds.max(0.0) * decoder.sample_rate as f64) as usize;
    let buffer = decoder.drain_frames(limit.max(1))?;
    Ok((buffer, start))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_buffer_reports_a_zero_duration() {
        let buffer = AudioBuffer {
            sample_rate: 48_000,
            channels: 2,
            samples: vec![Vec::new(), Vec::new()],
        };
        assert_eq!(buffer.frame_count(), 0);
        assert_eq!(buffer.duration_seconds(), 0.0);
    }

    #[test]
    fn duration_follows_the_sample_count() {
        let buffer = AudioBuffer {
            sample_rate: 1_000,
            channels: 1,
            samples: vec![vec![0.0; 2_500]],
        };
        assert_eq!(buffer.frame_count(), 2_500);
        assert!((buffer.duration_seconds() - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn a_zero_sample_rate_does_not_divide_by_zero() {
        let buffer = AudioBuffer {
            sample_rate: 0,
            channels: 1,
            samples: vec![vec![0.0; 10]],
        };
        assert!(buffer.duration_seconds().is_finite());
    }
}
