use std::ffi::{c_int, CString};
use std::path::Path;

use super::sys::{self, AVFormatContext, AVFrame, AVPacket, AVStream};
use super::{null_mut, null_void, Api};
use crate::color::{self, ColorSpec, Matrix, SpsColor};
use crate::decode::{DecodeError, Frame, VideoInfo};

pub(crate) const AV_TIME_BASE: f64 = 1_000_000.0;

pub fn spec_from_signalling(colorspace: c_int, color_range: c_int, height: usize) -> ColorSpec {
    let matrix = match colorspace {
        sys::AVCOL_SPC_BT709 => Some(Matrix::Bt709),
        sys::AVCOL_SPC_BT470BG | sys::AVCOL_SPC_SMPTE170M => Some(Matrix::Bt601),
        9 | 10 => Some(Matrix::Bt2020),
        _ => None,
    };
    let full_range = match color_range {
        sys::AVCOL_RANGE_JPEG => Some(true),
        sys::AVCOL_RANGE_MPEG => Some(false),
        _ => None,
    };

    match (matrix, full_range) {
        (None, None) => ColorSpec::assumed_for_height(height),
        (matrix, full_range) => color::resolve(
            Some(SpsColor {
                full_range: full_range.unwrap_or(false),
                matrix,
            }),
            height,
        ),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transfer {
    Sdr,
    Pq,
    Hlg,
    Unspecified,
}

impl Transfer {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sdr => "SDR",
            Self::Pq => "PQ (SMPTE ST 2084)",
            Self::Hlg => "HLG (ARIB STD-B67)",
            Self::Unspecified => "unspecified",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicRange {
    pub bit_depth: u32,
    pub transfer: Transfer,
    pub wide_gamut: bool,
}

impl DynamicRange {
    pub const SDR_8_BIT: Self = Self {
        bit_depth: 8,
        transfer: Transfer::Sdr,
        wide_gamut: false,
    };

    pub fn is_hdr(self) -> bool {
        matches!(self.transfer, Transfer::Pq | Transfer::Hlg)
    }

    pub fn is_high_bit_depth(self) -> bool {
        self.bit_depth > 8
    }

    pub fn is_reduced_by_decoding(self) -> bool {
        self.is_hdr() || self.is_high_bit_depth()
    }

    pub fn describe(self) -> String {
        let gamut = if self.wide_gamut { "BT.2020" } else { "BT.709" };
        if !self.is_reduced_by_decoding() {
            return format!("{}-bit {} {gamut}", self.bit_depth, self.transfer.name());
        }
        let mut losses = Vec::new();
        if self.is_high_bit_depth() {
            losses.push(format!("{}-bit is truncated to 8-bit", self.bit_depth));
        }
        if self.is_hdr() {
            losses.push(format!(
                "{} is not tone-mapped, so highlights read flat",
                self.transfer.name()
            ));
        }
        if self.wide_gamut {
            losses.push("BT.2020 primaries are not converted to BT.709".to_owned());
        }
        format!(
            "{}-bit {} {gamut}: {}",
            self.bit_depth,
            self.transfer.name(),
            losses.join("; ")
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalerKey {
    source_width: c_int,
    source_height: c_int,
    source_format: c_int,
    destination_width: c_int,
    destination_height: c_int,
    destination_format: c_int,
    flags: c_int,
}

struct Demuxer {
    api: &'static Api,
    format: *mut AVFormatContext,
    codec: *mut std::ffi::c_void,
    packet: *mut AVPacket,
    frame: *mut AVFrame,
    scaler: *mut std::ffi::c_void,
    scaled: Vec<u8>,
    scaler_key: Option<ScalerKey>,
    stream_index: c_int,
    time_base: f64,
    info: VideoInfo,
    dynamic_range: DynamicRange,
}

impl Drop for Demuxer {
    fn drop(&mut self) {
        unsafe {
            if !self.scaler.is_null() {
                (self.api.sws_freeContext)(self.scaler);
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

fn backend_error(detail: impl Into<String>) -> DecodeError {
    DecodeError::Backend(detail.into())
}

impl Demuxer {
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

        let mut demuxer = Self {
            api,
            format,
            codec: null_void(),
            packet: null_mut(),
            frame: null_mut(),
            scaler: null_void(),
            scaled: Vec::new(),
            scaler_key: None,
            stream_index: -1,
            time_base: 0.0,
            info: VideoInfo {
                width: 0,
                height: 0,
                duration_seconds: 0.0,
                frame_count: 0,
            },
            dynamic_range: DynamicRange::SDR_8_BIT,
        };

        if unsafe { (api.avformat_find_stream_info)(demuxer.format, null_mut()) } < 0 {
            return Err(DecodeError::Container("no stream info".into()));
        }

        demuxer.stream_index = unsafe {
            (api.av_find_best_stream)(
                demuxer.format,
                sys::AVMEDIA_TYPE_VIDEO,
                -1,
                -1,
                null_mut(),
                0,
            )
        };
        if demuxer.stream_index < 0 {
            return Err(DecodeError::NoVideoTrack);
        }

        let stream = demuxer.stream()?;
        let codecpar = unsafe { (*stream).codecpar };
        if codecpar.is_null() {
            return Err(DecodeError::NoVideoTrack);
        }

        demuxer.time_base = unsafe { (*stream).time_base }.as_f64();
        let duration_ticks = unsafe { (*demuxer.format).duration };
        let duration_seconds = if duration_ticks > 0 {
            duration_ticks as f64 / AV_TIME_BASE
        } else {
            0.0
        };
        let nb_frames = unsafe { (*stream).nb_frames };
        let average_rate = unsafe { (*stream).avg_frame_rate }.as_f64();
        let frame_count = if nb_frames > 0 {
            nb_frames as u32
        } else if average_rate > 0.0 && duration_seconds > 0.0 {
            (average_rate * duration_seconds).round() as u32
        } else {
            0
        };

        demuxer.info = VideoInfo {
            width: unsafe { (*codecpar).width }.max(0) as u16,
            height: unsafe { (*codecpar).height }.max(0) as u16,
            duration_seconds,
            frame_count,
        };

        demuxer.dynamic_range = unsafe { dynamic_range_of(api, codecpar) };

        let decoder = unsafe { (api.avcodec_find_decoder)((*codecpar).codec_id) };
        if decoder.is_null() {
            return Err(backend_error(format!(
                "no decoder for codec id {}",
                unsafe { (*codecpar).codec_id }
            )));
        }

        demuxer.codec = unsafe { (api.avcodec_alloc_context3)(decoder) };
        if demuxer.codec.is_null() {
            return Err(backend_error("avcodec_alloc_context3 failed"));
        }
        if unsafe { (api.avcodec_parameters_to_context)(demuxer.codec, codecpar) } < 0 {
            return Err(backend_error("avcodec_parameters_to_context failed"));
        }
        let opened = unsafe { (api.avcodec_open2)(demuxer.codec, decoder, null_mut()) };
        if opened < 0 {
            return Err(backend_error(format!("avcodec_open2 failed ({opened})")));
        }

        demuxer.packet = unsafe { (api.av_packet_alloc)() };
        demuxer.frame = unsafe { (api.av_frame_alloc)() };
        if demuxer.packet.is_null() || demuxer.frame.is_null() {
            return Err(backend_error("packet or frame allocation failed"));
        }

        Ok(demuxer)
    }

    fn stream(&self) -> Result<*mut AVStream, DecodeError> {
        let count = unsafe { (*self.format).nb_streams } as c_int;
        if self.stream_index < 0 || self.stream_index >= count {
            return Err(DecodeError::NoVideoTrack);
        }
        Ok(unsafe { *(*self.format).streams.add(self.stream_index as usize) })
    }

    fn seek(&mut self, seconds: f64) -> Result<(), DecodeError> {
        let target = if self.time_base > 0.0 {
            (seconds / self.time_base) as i64
        } else {
            (seconds * AV_TIME_BASE) as i64
        };
        let sought = unsafe {
            (self.api.avformat_seek_file)(
                self.format,
                self.stream_index,
                i64::MIN,
                target,
                target,
                sys::AVSEEK_FLAG_BACKWARD,
            )
        };
        if sought < 0 {
            return Err(DecodeError::Container(format!(
                "avformat_seek_file failed ({sought}) seeking to {seconds:.6}s"
            )));
        }
        unsafe { (self.api.avcodec_flush_buffers)(self.codec) };
        Ok(())
    }

    fn feed_one_packet(&mut self) {
        loop {
            let read = unsafe { (self.api.av_read_frame)(self.format, self.packet) };
            if read < 0 {
                unsafe { (self.api.avcodec_send_packet)(self.codec, null_mut()) };
                return;
            }
            let index = unsafe { (*self.packet).stream_index };
            if index == self.stream_index {
                unsafe {
                    (self.api.avcodec_send_packet)(self.codec, self.packet);
                    (self.api.av_packet_unref)(self.packet);
                }
                return;
            }
            unsafe { (self.api.av_packet_unref)(self.packet) };
        }
    }

    fn next_decoded(&mut self) -> Result<bool, DecodeError> {
        loop {
            let received = unsafe { (self.api.avcodec_receive_frame)(self.codec, self.frame) };
            if received == 0 {
                return Ok(true);
            }
            if received == sys::AVERROR_EOF {
                return Ok(false);
            }
            if received != sys::AVERROR_EAGAIN {
                return Err(DecodeError::Decoder(format!(
                    "avcodec_receive_frame failed ({received})"
                )));
            }
            self.feed_one_packet();
        }
    }

    fn frame_timestamp(&self) -> f64 {
        let pts = unsafe { (*self.frame).pts };
        if pts == i64::MIN {
            return 0.0;
        }
        pts as f64 * self.time_base
    }

    fn to_rgba(&mut self, override_spec: Option<ColorSpec>) -> Result<Frame, DecodeError> {
        let width = unsafe { (*self.frame).width }.max(0) as usize;
        let height = unsafe { (*self.frame).height }.max(0) as usize;
        if width == 0 || height == 0 {
            return Err(DecodeError::NoFrame);
        }

        let spec = override_spec.unwrap_or_else(|| {
            spec_from_signalling(
                unsafe { (*self.frame).colorspace },
                unsafe { (*self.frame).color_range },
                height,
            )
        });

        let format = unsafe { (*self.frame).format };
        let mut rgba = vec![255u8; width * height * 4];

        if format == sys::AV_PIX_FMT_YUV420P {
            let (y, u, v, strides) = unsafe { self.planes(width, height) };
            color::i420_to_rgba(y, u, v, (width, height), strides, spec, &mut rgba);
        } else {
            self.rescale_to_yuv420p(width, height, format)?;
            let y_size = width * height;
            let chroma = width.div_ceil(2) * height.div_ceil(2);
            let (y, rest) = self.scaled.split_at(y_size);
            let (u, v) = rest.split_at(chroma);
            color::i420_to_rgba(
                y,
                u,
                v,
                (width, height),
                (width, width.div_ceil(2), width.div_ceil(2)),
                spec,
                &mut rgba,
            );
        }

        Ok(Frame {
            width,
            height,
            rgba,
        })
    }

    unsafe fn planes(
        &self,
        width: usize,
        height: usize,
    ) -> (&[u8], &[u8], &[u8], (usize, usize, usize)) {
        let y_stride = unsafe { (*self.frame).linesize[0] }.max(0) as usize;
        let u_stride = unsafe { (*self.frame).linesize[1] }.max(0) as usize;
        let v_stride = unsafe { (*self.frame).linesize[2] }.max(0) as usize;
        let chroma_height = height.div_ceil(2);

        let y =
            unsafe { std::slice::from_raw_parts((*self.frame).data[0], y_stride * height.max(1)) };
        let u = unsafe {
            std::slice::from_raw_parts((*self.frame).data[1], u_stride * chroma_height.max(1))
        };
        let v = unsafe {
            std::slice::from_raw_parts((*self.frame).data[2], v_stride * chroma_height.max(1))
        };
        let _ = width;

        (y, u, v, (y_stride, u_stride, v_stride))
    }

    fn rescale_to_yuv420p(
        &mut self,
        width: usize,
        height: usize,
        format: c_int,
    ) -> Result<(), DecodeError> {
        let key = ScalerKey {
            source_width: width as c_int,
            source_height: height as c_int,
            source_format: format,
            destination_width: width as c_int,
            destination_height: height as c_int,
            destination_format: sys::AV_PIX_FMT_YUV420P,
            flags: sys::SWS_BILINEAR,
        };
        if self.scaler.is_null() || self.scaler_key != Some(key) {
            if !self.scaler.is_null() {
                unsafe { (self.api.sws_freeContext)(self.scaler) };
                self.scaler = null_void();
            }
            self.scaler_key = None;
            self.scaler = unsafe {
                (self.api.sws_getContext)(
                    key.source_width,
                    key.source_height,
                    key.source_format,
                    key.destination_width,
                    key.destination_height,
                    key.destination_format,
                    key.flags,
                    null_void(),
                    null_void(),
                    std::ptr::null(),
                )
            };
            if self.scaler.is_null() {
                return Err(backend_error(format!(
                    "no swscale conversion from pixel format {format} to yuv420p"
                )));
            }
            self.scaler_key = Some(key);
        }
        let chroma_width = width.div_ceil(2);
        let chroma_height = height.div_ceil(2);
        self.scaled
            .resize(width * height + chroma_width * chroma_height * 2, 0);

        let (y, rest) = self.scaled.split_at_mut(width * height);
        let (u, v) = rest.split_at_mut(chroma_width * chroma_height);
        let destination: [*mut u8; 4] =
            [y.as_mut_ptr(), u.as_mut_ptr(), v.as_mut_ptr(), null_mut()];
        let destination_stride: [c_int; 4] = [
            width as c_int,
            chroma_width as c_int,
            chroma_width as c_int,
            0,
        ];
        let source: [*const u8; 4] = unsafe {
            [
                (*self.frame).data[0],
                (*self.frame).data[1],
                (*self.frame).data[2],
                (*self.frame).data[3],
            ]
        };
        let source_stride: [c_int; 4] = unsafe {
            [
                (*self.frame).linesize[0],
                (*self.frame).linesize[1],
                (*self.frame).linesize[2],
                (*self.frame).linesize[3],
            ]
        };

        let converted = unsafe {
            (self.api.sws_scale)(
                self.scaler,
                source.as_ptr(),
                source_stride.as_ptr(),
                0,
                height as c_int,
                destination.as_ptr(),
                destination_stride.as_ptr(),
            )
        };
        if converted <= 0 {
            return Err(backend_error("sws_scale produced no rows"));
        }
        Ok(())
    }
}

fn transfer_from_trc(color_trc: c_int) -> Transfer {
    match color_trc {
        sys::AVCOL_TRC_SMPTE2084 => Transfer::Pq,
        sys::AVCOL_TRC_ARIB_STD_B67 => Transfer::Hlg,
        sys::AVCOL_TRC_BT709 | sys::AVCOL_TRC_BT2020_10 | sys::AVCOL_TRC_BT2020_12 => Transfer::Sdr,
        0 | 2 => Transfer::Unspecified,
        _ => Transfer::Sdr,
    }
}

fn is_wide_gamut(color_primaries: c_int, color_space: c_int) -> bool {
    color_primaries == sys::AVCOL_PRI_BT2020
        || matches!(
            color_space,
            sys::AVCOL_SPC_BT2020_NCL | sys::AVCOL_SPC_BT2020_CL
        )
}

unsafe fn dynamic_range_of(api: &Api, codecpar: *const sys::AVCodecParameters) -> DynamicRange {
    let format = unsafe { (*codecpar).format };
    let descriptor = unsafe { (api.av_pix_fmt_desc_get)(format) };
    let coded_depth = unsafe { (*codecpar).bits_per_raw_sample }.max(0) as u32;
    let bit_depth = if descriptor.is_null() {
        coded_depth.max(8)
    } else {
        let depth = unsafe { (*descriptor).comp[0].depth }.max(0) as u32;
        depth.max(coded_depth).max(8)
    };

    let transfer = transfer_from_trc(unsafe { (*codecpar).color_trc });
    let wide_gamut = is_wide_gamut(unsafe { (*codecpar).color_primaries }, unsafe {
        (*codecpar).color_space
    });

    DynamicRange {
        bit_depth,
        transfer,
        wide_gamut,
    }
}

pub fn dynamic_range(path: &Path) -> Result<DynamicRange, DecodeError> {
    Ok(Demuxer::open(path)?.dynamic_range)
}

pub fn probe(path: &Path) -> Result<VideoInfo, DecodeError> {
    let demuxer = Demuxer::open(path)?;
    Ok(VideoInfo {
        width: demuxer.info.width,
        height: demuxer.info.height,
        duration_seconds: demuxer.info.duration_seconds,
        frame_count: demuxer.info.frame_count,
    })
}

pub fn frame_at_with_color(
    path: &Path,
    seconds: f64,
    override_spec: Option<ColorSpec>,
) -> Result<Frame, DecodeError> {
    let mut stream = FfmpegStream::open(path)?;
    stream.frame_at_with_color(seconds, override_spec)
}

const TIMESTAMP_TOLERANCE: f64 = 1e-6;

const FORWARD_SEEK_SECONDS: f64 = 1.5;

pub struct FfmpegStream {
    demuxer: Demuxer,
    spec: Option<ColorSpec>,
    positioned: bool,
    current: Option<(f64, Frame)>,
    held: Option<(f64, Frame)>,
    seeks: u64,
    decoded_samples: u64,
}

impl FfmpegStream {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DecodeError> {
        Ok(Self {
            demuxer: Demuxer::open(path.as_ref())?,
            spec: None,
            positioned: false,
            current: None,
            held: None,
            seeks: 0,
            decoded_samples: 0,
        })
    }

    pub fn info(&self) -> &VideoInfo {
        &self.demuxer.info
    }

    pub fn dynamic_range(&self) -> DynamicRange {
        self.demuxer.dynamic_range
    }

    pub fn seek_count(&self) -> u64 {
        self.seeks
    }

    pub fn decoded_sample_count(&self) -> u64 {
        self.decoded_samples
    }

    pub fn last_timestamp(&self) -> f64 {
        self.current
            .as_ref()
            .map(|(timestamp, _)| *timestamp)
            .unwrap_or(0.0)
    }

    pub fn frame_at(&mut self, seconds: f64) -> Result<Frame, DecodeError> {
        self.frame_at_with_color(seconds, None)
    }

    fn next_frame(&mut self) -> Result<Option<(f64, Frame)>, DecodeError> {
        if !self.demuxer.next_decoded()? {
            return Ok(None);
        }
        let timestamp = self.demuxer.frame_timestamp();
        let frame = self.demuxer.to_rgba(self.spec)?;
        self.decoded_samples += 1;
        Ok(Some((timestamp, frame)))
    }

    pub fn frame_at_with_color(
        &mut self,
        seconds: f64,
        override_spec: Option<ColorSpec>,
    ) -> Result<Frame, DecodeError> {
        let target = seconds.max(0.0);
        if self.spec != override_spec {
            self.spec = override_spec;
            self.positioned = false;
        }

        let backwards = self
            .current
            .as_ref()
            .is_some_and(|(timestamp, _)| target + TIMESTAMP_TOLERANCE < *timestamp);

        let far_ahead = self
            .current
            .as_ref()
            .is_some_and(|(timestamp, _)| target > *timestamp + FORWARD_SEEK_SECONDS);
        if !self.positioned || backwards || far_ahead {
            self.demuxer.seek(target)?;
            self.seeks += 1;
            self.positioned = true;
            self.current = None;
            self.held = None;
        }

        loop {
            if let Some((timestamp, _)) = &self.held {
                if *timestamp > target + TIMESTAMP_TOLERANCE {
                    break;
                }
                self.current = self.held.take();
                continue;
            }
            match self.next_frame()? {
                Some(entry) => self.held = Some(entry),
                None => break,
            }
        }

        if self.current.is_none() {
            self.current = self.held.take();
        }

        self.current
            .as_ref()
            .map(|(_, frame)| frame.clone())
            .ok_or(DecodeError::NoFrame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pq_and_hlg_signalling_are_recognised_as_hdr() {
        for (trc, expected) in [
            (sys::AVCOL_TRC_SMPTE2084, Transfer::Pq),
            (sys::AVCOL_TRC_ARIB_STD_B67, Transfer::Hlg),
        ] {
            let range = DynamicRange {
                bit_depth: 10,
                transfer: transfer_from_trc(trc),
                wide_gamut: true,
            };
            assert_eq!(range.transfer, expected);
            assert!(range.is_hdr());
            assert!(range.is_reduced_by_decoding());
            assert!(
                range.describe().contains("not tone-mapped"),
                "the description must admit the missing tone mapping, got: {}",
                range.describe()
            );
        }
    }

    #[test]
    fn sdr_transfers_are_not_mistaken_for_hdr() {
        for trc in [
            sys::AVCOL_TRC_BT709,
            sys::AVCOL_TRC_BT2020_10,
            sys::AVCOL_TRC_BT2020_12,
        ] {
            assert_eq!(transfer_from_trc(trc), Transfer::Sdr);
        }
        assert_eq!(transfer_from_trc(0), Transfer::Unspecified);
        assert_eq!(transfer_from_trc(2), Transfer::Unspecified);
    }

    #[test]
    fn bt2020_is_wide_gamut_from_either_primaries_or_matrix() {
        assert!(is_wide_gamut(sys::AVCOL_PRI_BT2020, 2));
        assert!(is_wide_gamut(2, sys::AVCOL_SPC_BT2020_NCL));
        assert!(is_wide_gamut(2, sys::AVCOL_SPC_BT2020_CL));
        assert!(!is_wide_gamut(sys::AVCOL_PRI_BT709, sys::AVCOL_SPC_BT709));
    }

    #[test]
    fn an_eight_bit_sdr_stream_reports_no_loss() {
        let range = DynamicRange::SDR_8_BIT;
        assert!(!range.is_hdr());
        assert!(!range.is_high_bit_depth());
        assert!(!range.is_reduced_by_decoding());
        assert_eq!(range.describe(), "8-bit SDR BT.709");
    }

    #[test]
    fn a_ten_bit_pq_stream_names_every_loss_once() {
        let range = DynamicRange {
            bit_depth: 10,
            transfer: Transfer::Pq,
            wide_gamut: true,
        };
        let described = range.describe();
        assert!(described.contains("10-bit is truncated to 8-bit"));
        assert!(described.contains("PQ (SMPTE ST 2084) is not tone-mapped"));
        assert!(described.contains("BT.2020 primaries are not converted"));
    }

    #[test]
    fn bt709_signalling_maps_to_the_bt709_matrix() {
        let spec = spec_from_signalling(sys::AVCOL_SPC_BT709, sys::AVCOL_RANGE_MPEG, 1080);
        assert_eq!(spec.matrix, Matrix::Bt709);
        assert!(!spec.full_range);
    }

    #[test]
    fn bt601_signalling_maps_to_the_bt601_matrix() {
        for colorspace in [sys::AVCOL_SPC_BT470BG, sys::AVCOL_SPC_SMPTE170M] {
            let spec = spec_from_signalling(colorspace, sys::AVCOL_RANGE_MPEG, 480);
            assert_eq!(spec.matrix, Matrix::Bt601);
        }
    }

    #[test]
    fn full_range_signalling_is_carried_through() {
        let spec = spec_from_signalling(sys::AVCOL_SPC_BT709, sys::AVCOL_RANGE_JPEG, 720);
        assert!(spec.full_range);
    }

    #[test]
    fn unspecified_signalling_falls_back_to_the_height_assumption() {
        let unspecified = spec_from_signalling(2, 0, 1080);
        assert_eq!(unspecified, ColorSpec::assumed_for_height(1080));
        let small = spec_from_signalling(2, 0, 480);
        assert_eq!(small, ColorSpec::assumed_for_height(480));
    }

    #[test]
    fn an_unspecified_matrix_still_honours_an_explicit_range() {
        let spec = spec_from_signalling(2, sys::AVCOL_RANGE_JPEG, 1080);
        assert!(spec.full_range);
        assert_eq!(spec.matrix, Matrix::assumed_for_height(1080));
    }
}
