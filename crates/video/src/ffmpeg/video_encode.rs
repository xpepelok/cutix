use std::ffi::{CString, c_int, c_void};
use std::sync::OnceLock;

use super::sys::{self, AVCodecParameters, AVFrame, AVPacket};
use super::{Api, EncoderApi, null_mut, set_option, set_option_int};

pub const CANDIDATES: &[&str] = &[
    "h264_nvenc",
    "h264_qsv",
    "h264_amf",
    "h264_videotoolbox",
    "libx264",
    "libopenh264",
];

pub const HARDWARE: &[&str] = &["h264_nvenc", "h264_qsv", "h264_amf", "h264_videotoolbox"];

const AV_PKT_FLAG_KEY: c_int = 1;

const PROBE_WIDTH: u32 = 640;
const PROBE_HEIGHT: u32 = 360;
const PROBE_BITRATE: u32 = 1_000_000;

#[derive(Clone, Debug)]
pub struct EncodedPacket {
    pub bytes: Vec<u8>,
    pub is_sync: bool,
}

pub struct H264Encoder {
    api: &'static Api,
    encoder: &'static EncoderApi,
    name: &'static str,
    context: *mut c_void,
    frame: *mut AVFrame,
    packet: *mut AVPacket,
    width: u32,
    height: u32,
    pixel_format: c_int,
    interleaved: Vec<u8>,
    pts: i64,
}

unsafe impl Send for H264Encoder {}

impl Drop for H264Encoder {
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

pub fn is_hardware(name: &str) -> bool {
    HARDWARE.contains(&name)
}

fn canonical(name: &str) -> Option<&'static str> {
    CANDIDATES.iter().copied().find(|entry| *entry == name)
}

static AVAILABLE: OnceLock<Vec<&'static str>> = OnceLock::new();

pub fn available() -> &'static [&'static str] {
    AVAILABLE.get_or_init(|| {
        CANDIDATES
            .iter()
            .copied()
            .filter(|name| {
                H264Encoder::open(name, PROBE_WIDTH, PROBE_HEIGHT, (30, 1), PROBE_BITRATE).is_ok()
            })
            .collect()
    })
}

pub fn best() -> Option<&'static str> {
    available().first().copied()
}

impl H264Encoder {
    pub fn open(
        name: &str,
        width: u32,
        height: u32,
        frame_rate: (u32, u32),
        bitrate_bps: u32,
    ) -> Result<Self, String> {
        let name = canonical(name).ok_or_else(|| format!("{name} is not a known h264 encoder"))?;
        if width == 0 || height == 0 || width % 2 == 1 || height % 2 == 1 {
            return Err(format!("{width}x{height} is not an even, non-zero raster"));
        }
        let (numerator, denominator) = frame_rate;
        if numerator == 0 || denominator == 0 {
            return Err("the frame rate has a zero term".to_owned());
        }

        let planar = Self::open_as(
            name,
            sys::AV_PIX_FMT_YUV420P,
            width,
            height,
            frame_rate,
            bitrate_bps,
        );
        match planar {
            Ok(encoder) => Ok(encoder),
            Err(planar_error) => Self::open_as(
                name,
                sys::AV_PIX_FMT_NV12,
                width,
                height,
                frame_rate,
                bitrate_bps,
            )
            .map_err(|_| planar_error),
        }
    }

    fn open_as(
        name: &'static str,
        pixel_format: c_int,
        width: u32,
        height: u32,
        frame_rate: (u32, u32),
        bitrate_bps: u32,
    ) -> Result<Self, String> {
        let (numerator, denominator) = frame_rate;

        let ffmpeg = super::instance()?;
        let api = ffmpeg.api();
        let encoder = ffmpeg
            .encoder_api()
            .ok_or("this FFmpeg build exposes no encoder entry points")?;

        let symbol = CString::new(name).expect("literal");
        let codec = unsafe { (encoder.avcodec_find_encoder_by_name)(symbol.as_ptr()) };
        if codec.is_null() {
            return Err(format!("this FFmpeg build has no {name} encoder"));
        }

        let context = unsafe { (api.avcodec_alloc_context3)(codec) };
        if context.is_null() {
            return Err("avcodec_alloc_context3 failed".to_owned());
        }

        let mut this = Self {
            api,
            encoder,
            name,
            context,
            frame: null_mut(),
            packet: null_mut(),
            width,
            height,
            pixel_format,
            interleaved: Vec::new(),
            pts: 0,
        };

        let parameters = Parameters {
            encoder,
            raw: unsafe { (encoder.avcodec_parameters_alloc)() },
        };
        if parameters.raw.is_null() {
            return Err("avcodec_parameters_alloc failed".to_owned());
        }
        unsafe {
            (*parameters.raw).codec_type = sys::AVMEDIA_TYPE_VIDEO;
            (*parameters.raw).format = pixel_format;
            (*parameters.raw).width = width as c_int;
            (*parameters.raw).height = height as c_int;
            (*parameters.raw).bit_rate = i64::from(bitrate_bps);
        }
        let applied = unsafe { (api.avcodec_parameters_to_context)(this.context, parameters.raw) };
        if applied < 0 {
            return Err(format!("avcodec_parameters_to_context failed ({applied})"));
        }
        drop(parameters);

        let time_base = format!("{denominator}/{numerator}");
        if !set_option(api, this.context, "time_base", &time_base) {
            return Err("this FFmpeg build refuses time_base as an option".to_owned());
        }
        let fps = f64::from(numerator) / f64::from(denominator);
        set_option_int(api, this.context, "g", (fps.round() as i64).clamp(1, 300));
        set_option_int(api, this.context, "bf", 0);
        set_option_int(api, this.context, "threads", 0);

        let opened = unsafe { (api.avcodec_open2)(this.context, codec, null_mut()) };
        if opened < 0 {
            return Err(format!(
                "avcodec_open2 failed for {name} at {width}x{height} ({opened})"
            ));
        }

        this.frame = unsafe { (api.av_frame_alloc)() };
        this.packet = unsafe { (api.av_packet_alloc)() };
        if this.frame.is_null() || this.packet.is_null() {
            return Err("frame or packet allocation failed".to_owned());
        }

        Ok(this)
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    fn drain(&mut self, into: &mut Vec<EncodedPacket>) -> Result<(), String> {
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
                into.push(EncodedPacket {
                    bytes: unsafe { std::slice::from_raw_parts(data, size) }.to_vec(),
                    is_sync: unsafe { (*self.packet).flags } & AV_PKT_FLAG_KEY != 0,
                });
            }
            unsafe { (self.api.av_packet_unref)(self.packet) };
        }
    }

    pub fn encode(&mut self, y: &[u8], u: &[u8], v: &[u8]) -> Result<Vec<EncodedPacket>, String> {
        let width = self.width as usize;
        let height = self.height as usize;
        let chroma_width = width / 2;
        let chroma_height = height / 2;
        if y.len() < width * height
            || u.len() < chroma_width * chroma_height
            || v.len() < chroma_width * chroma_height
        {
            return Err("the planes are smaller than the configured raster".to_owned());
        }

        if self.pixel_format == sys::AV_PIX_FMT_NV12 {
            self.interleaved.resize(chroma_width * chroma_height * 2, 0);
            for (pair, (blue, red)) in self
                .interleaved
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .zip(u.iter().zip(v.iter()))
            {
                pair[0] = *blue;
                pair[1] = *red;
            }
        }

        unsafe {
            (self.api.av_frame_unref)(self.frame);
            (*self.frame).format = self.pixel_format;
            (*self.frame).width = self.width as c_int;
            (*self.frame).height = self.height as c_int;
        }
        let allocated = unsafe { (self.encoder.av_frame_get_buffer)(self.frame, 0) };
        if allocated < 0 {
            return Err(format!("av_frame_get_buffer failed ({allocated})"));
        }

        unsafe {
            copy_plane(
                (*self.frame).data[0],
                (*self.frame).linesize[0],
                y,
                width,
                height,
            );
            if self.pixel_format == sys::AV_PIX_FMT_NV12 {
                copy_plane(
                    (*self.frame).data[1],
                    (*self.frame).linesize[1],
                    &self.interleaved,
                    chroma_width * 2,
                    chroma_height,
                );
            } else {
                copy_plane(
                    (*self.frame).data[1],
                    (*self.frame).linesize[1],
                    u,
                    chroma_width,
                    chroma_height,
                );
                copy_plane(
                    (*self.frame).data[2],
                    (*self.frame).linesize[2],
                    v,
                    chroma_width,
                    chroma_height,
                );
            }
            (*self.frame).pts = self.pts;
        }
        self.pts += 1;

        let sent = unsafe { (self.encoder.avcodec_send_frame)(self.context, self.frame) };
        if sent < 0 {
            return Err(format!("avcodec_send_frame failed ({sent})"));
        }
        let mut packets = Vec::new();
        self.drain(&mut packets)?;
        Ok(packets)
    }

    pub fn finish(&mut self) -> Result<Vec<EncodedPacket>, String> {
        let sent = unsafe { (self.encoder.avcodec_send_frame)(self.context, std::ptr::null()) };
        if sent < 0 {
            return Err(format!("flushing {} failed ({sent})", self.name));
        }
        let mut packets = Vec::new();
        self.drain(&mut packets)?;
        Ok(packets)
    }
}

unsafe fn copy_plane(target: *mut u8, stride: c_int, source: &[u8], width: usize, height: usize) {
    if target.is_null() || stride <= 0 {
        return;
    }
    let stride = stride as usize;
    for row in 0..height {
        unsafe {
            std::ptr::copy_nonoverlapping(
                source.as_ptr().add(row * width),
                target.add(row * stride),
                width,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hardware_name_is_also_a_candidate() {
        for name in HARDWARE {
            assert!(CANDIDATES.contains(name), "{name} is not in the probe list");
        }
    }

    #[test]
    fn the_software_fallback_is_probed_last() {
        assert_eq!(CANDIDATES.last(), Some(&"libopenh264"));
        assert!(!is_hardware("libx264"));
    }

    #[test]
    fn an_unknown_encoder_name_is_refused_before_ffmpeg_is_touched() {
        assert!(H264Encoder::open("h265_magic", 640, 360, (30, 1), 1_000_000).is_err());
    }

    #[test]
    fn an_odd_raster_is_refused() {
        assert!(H264Encoder::open("libx264", 641, 360, (30, 1), 1_000_000).is_err());
        assert!(H264Encoder::open("libx264", 640, 0, (30, 1), 1_000_000).is_err());
    }

    #[test]
    fn a_zero_frame_rate_is_refused() {
        assert!(H264Encoder::open("libx264", 640, 360, (0, 1), 1_000_000).is_err());
        assert!(H264Encoder::open("libx264", 640, 360, (30, 0), 1_000_000).is_err());
    }

    #[test]
    fn probing_is_stable_across_calls() {
        assert_eq!(available(), available());
        if let Some(name) = best() {
            assert!(CANDIDATES.contains(&name));
        }
    }
}
