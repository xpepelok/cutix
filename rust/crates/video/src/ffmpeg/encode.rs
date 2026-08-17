use std::ffi::{c_int, c_void, CString};
use std::sync::OnceLock;

use super::sys::{self, AVChannelLayout, AVCodecParameters, AVFrame, AVPacket};
use super::{null_mut, null_void, Api, EncoderApi};

pub const AAC_FRAME_SAMPLES: usize = 1024;

pub struct AacEncoder {
    api: &'static Api,
    encoder: &'static EncoderApi,
    context: *mut c_void,
    frame: *mut AVFrame,
    packet: *mut AVPacket,
    planes: Vec<Vec<f32>>,
    channels: usize,
    layout_offset: usize,
    pts: i64,
    packets: Vec<Vec<u8>>,
}

impl Drop for AacEncoder {
    fn drop(&mut self) {
        unsafe {
            if !self.packet.is_null() {
                (self.api.av_packet_free)(&mut self.packet);
            }
            if !self.frame.is_null() {
                (self.api.av_frame_free)(&mut self.frame);
            }
            if !self.context.is_null() {
                (self.api.avcodec_free_context)(&mut self.context);
            }
        }
    }
}

struct Parameters {
    encoder: &'static EncoderApi,
    raw: *mut AVCodecParameters,
}

impl Drop for Parameters {
    fn drop(&mut self) {
        unsafe {
            if !self.raw.is_null() {
                (self.encoder.avcodec_parameters_free)(&mut self.raw);
            }
        }
    }
}

pub fn layout_for(channels: usize) -> AVChannelLayout {
    let mask = match channels {
        1 => sys::AV_CH_LAYOUT_MONO,
        2 => sys::AV_CH_LAYOUT_STEREO,
        other if other < 64 => (1u64 << other) - 1,
        _ => 0,
    };
    AVChannelLayout {
        order: sys::AV_CHANNEL_ORDER_NATIVE,
        nb_channels: channels as c_int,
        u: mask,
        opaque: null_void(),
    }
}

const PROBE_WORDS: usize = 512;
const FIRST_CANDIDATE: usize = 304;
const LAST_CANDIDATE: usize = 2048;

static CH_LAYOUT_OFFSET: OnceLock<Option<usize>> = OnceLock::new();

pub fn frame_channel_layout_offset() -> Option<usize> {
    ch_layout_offset()
}

fn ch_layout_offset() -> Option<usize> {
    *CH_LAYOUT_OFFSET.get_or_init(|| {
        let ffmpeg = super::instance().ok()?;
        let api = ffmpeg.api();
        let encoder = ffmpeg.encoder_api()?;

        let mut storage = vec![0u64; PROBE_WORDS];
        let frame = storage.as_mut_ptr().cast::<AVFrame>();
        let bytes = storage.as_mut_ptr().cast::<u8>();
        let wanted = layout_for(2);

        unsafe {
            (*frame).format = sys::AV_SAMPLE_FMT_FLTP;
            (*frame).nb_samples = AAC_FRAME_SAMPLES as c_int;
            (*frame).sample_rate = 48_000;
        }

        for offset in (FIRST_CANDIDATE..LAST_CANDIDATE).step_by(8) {
            let slot = unsafe { bytes.add(offset).cast::<AVChannelLayout>() };
            let saved = unsafe { std::ptr::read_unaligned(slot) };
            unsafe { std::ptr::write_unaligned(slot, wanted) };
            let allocated = unsafe { (encoder.av_frame_get_buffer)(frame, 0) };
            if allocated == 0 {
                unsafe { (api.av_frame_unref)(frame) };
                return Some(offset);
            }
            unsafe { std::ptr::write_unaligned(slot, saved) };
        }
        None
    })
}

impl AacEncoder {
    pub fn new(channels: usize, sample_rate: u32, bitrate_bps: u32) -> Result<Self, String> {
        let ffmpeg = super::instance()?;
        let api = ffmpeg.api();
        let encoder = ffmpeg
            .encoder_api()
            .ok_or("this FFmpeg build exposes no encoder entry points")?;

        let name = CString::new("aac").expect("literal");
        let codec = unsafe { (encoder.avcodec_find_encoder_by_name)(name.as_ptr()) };
        if codec.is_null() {
            return Err("this FFmpeg build has no aac encoder".to_owned());
        }

        let context = unsafe { (api.avcodec_alloc_context3)(codec) };
        if context.is_null() {
            return Err("avcodec_alloc_context3 failed".to_owned());
        }

        let channels = channels.max(1);
        let layout_offset =
            ch_layout_offset().ok_or("could not locate AVFrame::ch_layout in this FFmpeg build")?;
        let mut this = Self {
            api,
            encoder,
            context,
            frame: null_mut(),
            packet: null_mut(),
            planes: vec![vec![0.0; AAC_FRAME_SAMPLES]; channels],
            channels,
            layout_offset,
            pts: 0,
            packets: Vec::new(),
        };

        let parameters = Parameters {
            encoder,
            raw: unsafe { (encoder.avcodec_parameters_alloc)() },
        };
        if parameters.raw.is_null() {
            return Err("avcodec_parameters_alloc failed".to_owned());
        }
        unsafe {
            (*parameters.raw).codec_type = sys::AVMEDIA_TYPE_AUDIO;
            (*parameters.raw).format = sys::AV_SAMPLE_FMT_FLTP;
            (*parameters.raw).sample_rate = sample_rate as c_int;
            (*parameters.raw).bit_rate = i64::from(bitrate_bps);
            (*parameters.raw).ch_layout = layout_for(channels);
        }
        let applied = unsafe { (api.avcodec_parameters_to_context)(this.context, parameters.raw) };
        if applied < 0 {
            return Err(format!("avcodec_parameters_to_context failed ({applied})"));
        }
        drop(parameters);

        let opened = unsafe { (api.avcodec_open2)(this.context, codec, null_mut()) };
        if opened < 0 {
            return Err(format!(
                "avcodec_open2 failed for the aac encoder at {sample_rate} Hz, {channels} ch ({opened})"
            ));
        }

        this.frame = unsafe { (api.av_frame_alloc)() };
        this.packet = unsafe { (api.av_packet_alloc)() };
        if this.frame.is_null() || this.packet.is_null() {
            return Err("frame or packet allocation failed".to_owned());
        }
        unsafe {
            (*this.frame).format = sys::AV_SAMPLE_FMT_FLTP;
            (*this.frame).sample_rate = sample_rate as c_int;
            let slot = this
                .frame
                .cast::<u8>()
                .add(this.layout_offset)
                .cast::<AVChannelLayout>();
            std::ptr::write_unaligned(slot, layout_for(channels));
        }

        Ok(this)
    }

    fn drain(&mut self) -> Result<(), String> {
        loop {
            let received =
                unsafe { (self.encoder.avcodec_receive_packet)(self.context, self.packet) };
            if received == sys::AVERROR_EAGAIN || received == sys::AVERROR_EOF {
                return Ok(());
            }
            if received < 0 {
                return Err(format!("avcodec_receive_packet failed ({received})"));
            }
            let size = unsafe { (*self.packet).size }.max(0) as usize;
            let data = unsafe { (*self.packet).data };
            if size > 0 && !data.is_null() {
                self.packets
                    .push(unsafe { std::slice::from_raw_parts(data, size) }.to_vec());
            }
            unsafe { (self.api.av_packet_unref)(self.packet) };
        }
    }

    fn send_block(&mut self, block: &[f32]) -> Result<(), String> {
        let samples = block.len() / self.channels;
        if samples == 0 {
            return Ok(());
        }
        for (channel, plane) in self.planes.iter_mut().enumerate() {
            for (index, slot) in plane.iter_mut().take(samples).enumerate() {
                *slot = block[index * self.channels + channel];
            }
        }

        unsafe {
            (*self.frame).nb_samples = samples as c_int;
            (*self.frame).pts = self.pts;
            (*self.frame).linesize[0] = (samples * std::mem::size_of::<f32>()) as c_int;
            for index in 0..sys::AV_NUM_DATA_POINTERS {
                (*self.frame).data[index] = match self.planes.get_mut(index) {
                    Some(plane) => plane.as_mut_ptr().cast::<u8>(),
                    None => null_mut(),
                };
            }
            (*self.frame).extended_data = (*self.frame).data.as_mut_ptr();
        }
        self.pts += samples as i64;

        let sent = unsafe { (self.encoder.avcodec_send_frame)(self.context, self.frame) };
        if sent < 0 {
            return Err(format!("avcodec_send_frame failed ({sent})"));
        }
        self.drain()
    }

    pub fn finish(mut self) -> Result<Vec<Vec<u8>>, String> {
        let sent = unsafe { (self.encoder.avcodec_send_frame)(self.context, std::ptr::null()) };
        if sent < 0 {
            return Err(format!("flushing the aac encoder failed ({sent})"));
        }
        self.drain()?;
        Ok(std::mem::take(&mut self.packets))
    }
}

pub fn encode_aac(
    interleaved: &[f32],
    channels: usize,
    sample_rate: u32,
    bitrate_bps: u32,
) -> Result<Vec<Vec<u8>>, String> {
    let channels = channels.max(1);
    let mut encoder = AacEncoder::new(channels, sample_rate, bitrate_bps)?;
    for block in interleaved.chunks(AAC_FRAME_SAMPLES * channels) {
        encoder.send_block(block)?;
    }
    encoder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stereo_layout_is_front_left_and_right() {
        let layout = layout_for(2);
        assert_eq!(layout.nb_channels, 2);
        assert_eq!(layout.u, sys::AV_CH_LAYOUT_STEREO);
        assert_eq!(layout.order, sys::AV_CHANNEL_ORDER_NATIVE);
    }

    #[test]
    fn a_mono_layout_is_the_centre_channel() {
        assert_eq!(layout_for(1).u, sys::AV_CH_LAYOUT_MONO);
    }
}
