use std::ffi::{c_char, c_int, c_uint, c_void};

pub const AVMEDIA_TYPE_VIDEO: c_int = 0;
pub const AVMEDIA_TYPE_AUDIO: c_int = 1;

pub const AV_NUM_DATA_POINTERS: usize = 8;

pub const AVERROR_EAGAIN: c_int = -11;
pub const AVERROR_EOF: c_int =
    -(('E' as c_int) | (('O' as c_int) << 8) | (('F' as c_int) << 16) | ((' ' as c_int) << 24));

pub const AVERROR_OPTION_NOT_FOUND: c_int =
    -(0xF8 | (('O' as c_int) << 8) | (('P' as c_int) << 16) | (('T' as c_int) << 24));

pub const AV_PIX_FMT_YUV420P: c_int = 0;

pub const AV_SAMPLE_FMT_FLTP: c_int = 8;

pub const AV_CHANNEL_ORDER_NATIVE: c_int = 1;
pub const AV_CH_LAYOUT_MONO: u64 = 0x4;
pub const AV_CH_LAYOUT_STEREO: u64 = 0x3;

pub const SWS_BILINEAR: c_int = 2;

pub const AVSEEK_FLAG_BACKWARD: c_int = 1;

pub const AVCOL_RANGE_MPEG: c_int = 1;
pub const AVCOL_RANGE_JPEG: c_int = 2;

pub const AVCOL_SPC_BT709: c_int = 1;
pub const AVCOL_SPC_BT470BG: c_int = 5;
pub const AVCOL_SPC_SMPTE170M: c_int = 6;
pub const AVCOL_SPC_BT2020_NCL: c_int = 9;
pub const AVCOL_SPC_BT2020_CL: c_int = 10;

pub const AVCOL_PRI_BT709: c_int = 1;
pub const AVCOL_PRI_BT2020: c_int = 9;

pub const AVCOL_TRC_BT709: c_int = 1;
pub const AVCOL_TRC_BT2020_10: c_int = 14;
pub const AVCOL_TRC_BT2020_12: c_int = 15;
pub const AVCOL_TRC_SMPTE2084: c_int = 16;
pub const AVCOL_TRC_ARIB_STD_B67: c_int = 18;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AVRational {
    pub num: c_int,
    pub den: c_int,
}

impl AVRational {
    pub fn as_f64(self) -> f64 {
        if self.den == 0 {
            return 0.0;
        }
        f64::from(self.num) / f64::from(self.den)
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AVChannelLayout {
    pub order: c_int,
    pub nb_channels: c_int,
    pub u: u64,
    pub opaque: *mut c_void,
}

impl AVChannelLayout {
    pub const EMPTY: Self = Self {
        order: 0,
        nb_channels: 0,
        u: 0,
        opaque: std::ptr::null_mut(),
    };
}

#[repr(C)]
pub struct AVFrame {
    pub data: [*mut u8; AV_NUM_DATA_POINTERS],
    pub linesize: [c_int; AV_NUM_DATA_POINTERS],
    pub extended_data: *mut *mut u8,
    pub width: c_int,
    pub height: c_int,
    pub nb_samples: c_int,
    pub format: c_int,
    pub pict_type: c_int,
    pub sample_aspect_ratio: AVRational,
    pub pts: i64,
    pub pkt_dts: i64,
    pub time_base: AVRational,
    pub quality: c_int,
    pub opaque: *mut c_void,
    pub repeat_pict: c_int,
    pub sample_rate: c_int,
    pub buf: [*mut c_void; AV_NUM_DATA_POINTERS],
    pub extended_buf: *mut *mut c_void,
    pub nb_extended_buf: c_int,
    pub side_data: *mut *mut c_void,
    pub nb_side_data: c_int,
    pub flags: c_int,
    pub color_range: c_int,
    pub color_primaries: c_int,
    pub color_trc: c_int,
    pub colorspace: c_int,
    pub chroma_location: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AVComponentDescriptor {
    pub plane: c_int,
    pub step: c_int,
    pub offset: c_int,
    pub shift: c_int,
    pub depth: c_int,
}

#[repr(C)]
pub struct AVPixFmtDescriptor {
    pub name: *const c_char,
    pub nb_components: u8,
    pub log2_chroma_w: u8,
    pub log2_chroma_h: u8,
    pub flags: u64,
    pub comp: [AVComponentDescriptor; 4],
    pub alias: *const c_char,
}

#[repr(C)]
pub struct AVCodecParameters {
    pub codec_type: c_int,
    pub codec_id: c_int,
    pub codec_tag: u32,
    pub extradata: *mut u8,
    pub extradata_size: c_int,
    pub coded_side_data: *mut c_void,
    pub nb_coded_side_data: c_int,
    pub format: c_int,
    pub bit_rate: i64,
    pub bits_per_coded_sample: c_int,
    pub bits_per_raw_sample: c_int,
    pub profile: c_int,
    pub level: c_int,
    pub width: c_int,
    pub height: c_int,
    pub sample_aspect_ratio: AVRational,
    pub framerate: AVRational,
    pub field_order: c_int,
    pub color_range: c_int,
    pub color_primaries: c_int,
    pub color_trc: c_int,
    pub color_space: c_int,
    pub chroma_location: c_int,
    pub video_delay: c_int,
    pub ch_layout: AVChannelLayout,
    pub sample_rate: c_int,
    pub block_align: c_int,
    pub frame_size: c_int,
    pub initial_padding: c_int,
}

#[repr(C)]
pub struct AVStream {
    pub av_class: *const c_void,
    pub index: c_int,
    pub id: c_int,
    pub codecpar: *mut AVCodecParameters,
    pub priv_data: *mut c_void,
    pub time_base: AVRational,
    pub start_time: i64,
    pub duration: i64,
    pub nb_frames: i64,
    pub disposition: c_int,
    pub discard: c_int,
    pub sample_aspect_ratio: AVRational,
    pub metadata: *mut c_void,
    pub avg_frame_rate: AVRational,
}

#[repr(C)]
pub struct AVFormatContext {
    pub av_class: *const c_void,
    pub iformat: *const c_void,
    pub oformat: *const c_void,
    pub priv_data: *mut c_void,
    pub pb: *mut c_void,
    pub ctx_flags: c_int,
    pub nb_streams: c_uint,
    pub streams: *mut *mut AVStream,
    pub nb_stream_groups: c_uint,
    pub stream_groups: *mut *mut c_void,
    pub nb_chapters: c_uint,
    pub chapters: *mut *mut c_void,
    pub url: *mut c_char,
    pub start_time: i64,
    pub duration: i64,
}

pub type AvFrameAlloc = unsafe extern "C" fn() -> *mut AVFrame;
pub type AvFrameFree = unsafe extern "C" fn(*mut *mut AVFrame);
pub type AvFrameUnref = unsafe extern "C" fn(*mut AVFrame);
pub type AvcodecVersion = unsafe extern "C" fn() -> c_uint;
pub type AvformatOpenInput = unsafe extern "C" fn(
    *mut *mut AVFormatContext,
    *const c_char,
    *const c_void,
    *mut *mut c_void,
) -> c_int;
pub type AvformatCloseInput = unsafe extern "C" fn(*mut *mut AVFormatContext);
pub type AvformatFindStreamInfo =
    unsafe extern "C" fn(*mut AVFormatContext, *mut *mut c_void) -> c_int;
pub type AvReadFrame = unsafe extern "C" fn(*mut AVFormatContext, *mut AVPacket) -> c_int;
pub type AvformatSeekFile =
    unsafe extern "C" fn(*mut AVFormatContext, c_int, i64, i64, i64, c_int) -> c_int;
pub type AvFindBestStream = unsafe extern "C" fn(
    *mut AVFormatContext,
    c_int,
    c_int,
    c_int,
    *mut *const c_void,
    c_int,
) -> c_int;
pub type AvcodecFindDecoder = unsafe extern "C" fn(c_int) -> *const c_void;
pub type AvPixFmtDescGet = unsafe extern "C" fn(c_int) -> *const AVPixFmtDescriptor;
pub type AvcodecAllocContext3 = unsafe extern "C" fn(*const c_void) -> *mut c_void;
pub type AvcodecFreeContext = unsafe extern "C" fn(*mut *mut c_void);
pub type AvcodecParametersToContext =
    unsafe extern "C" fn(*mut c_void, *const AVCodecParameters) -> c_int;
pub type AvcodecOpen2 = unsafe extern "C" fn(*mut c_void, *const c_void, *mut *mut c_void) -> c_int;
pub type AvcodecSendPacket = unsafe extern "C" fn(*mut c_void, *const AVPacket) -> c_int;
pub type AvcodecReceiveFrame = unsafe extern "C" fn(*mut c_void, *mut AVFrame) -> c_int;
pub type AvcodecFlushBuffers = unsafe extern "C" fn(*mut c_void);
pub type AvPacketAlloc = unsafe extern "C" fn() -> *mut AVPacket;
pub type AvPacketFree = unsafe extern "C" fn(*mut *mut AVPacket);
pub type AvPacketUnref = unsafe extern "C" fn(*mut AVPacket);
pub type SwsGetContext = unsafe extern "C" fn(
    c_int,
    c_int,
    c_int,
    c_int,
    c_int,
    c_int,
    c_int,
    *mut c_void,
    *mut c_void,
    *const f64,
) -> *mut c_void;
pub type SwsScale = unsafe extern "C" fn(
    *mut c_void,
    *const *const u8,
    *const c_int,
    c_int,
    c_int,
    *const *mut u8,
    *const c_int,
) -> c_int;
pub type SwsFreeContext = unsafe extern "C" fn(*mut c_void);
pub type SwrAllocSetOpts2 = unsafe extern "C" fn(
    *mut *mut c_void,
    *const AVChannelLayout,
    c_int,
    c_int,
    *const AVChannelLayout,
    c_int,
    c_int,
    c_int,
    *mut c_void,
) -> c_int;
pub type SwrInit = unsafe extern "C" fn(*mut c_void) -> c_int;
pub type SwrConvert =
    unsafe extern "C" fn(*mut c_void, *mut *mut u8, c_int, *const *const u8, c_int) -> c_int;
pub type SwrFree = unsafe extern "C" fn(*mut *mut c_void);

pub type AvcodecFindEncoderByName = unsafe extern "C" fn(*const c_char) -> *const c_void;
pub type AvcodecSendFrame = unsafe extern "C" fn(*mut c_void, *const AVFrame) -> c_int;
pub type AvcodecReceivePacket = unsafe extern "C" fn(*mut c_void, *mut AVPacket) -> c_int;
pub type AvcodecParametersAlloc = unsafe extern "C" fn() -> *mut AVCodecParameters;
pub type AvcodecParametersFree = unsafe extern "C" fn(*mut *mut AVCodecParameters);
pub type AvFrameGetBuffer = unsafe extern "C" fn(*mut AVFrame, c_int) -> c_int;

#[repr(C)]
pub struct AVPacket {
    pub buf: *mut c_void,
    pub pts: i64,
    pub dts: i64,
    pub data: *mut u8,
    pub size: c_int,
    pub stream_index: c_int,
    pub flags: c_int,
    pub side_data: *mut c_void,
    pub side_data_elems: c_int,
    pub duration: i64,
    pub pos: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    #[test]
    fn avframe_prefix_matches_the_c_layout() {
        assert_eq!(offset_of!(AVFrame, data), 0);
        assert_eq!(offset_of!(AVFrame, linesize), 64);
        assert_eq!(offset_of!(AVFrame, extended_data), 96);
        assert_eq!(offset_of!(AVFrame, width), 104);
        assert_eq!(offset_of!(AVFrame, height), 108);
        assert_eq!(offset_of!(AVFrame, nb_samples), 112);
        assert_eq!(offset_of!(AVFrame, format), 116);
        assert_eq!(offset_of!(AVFrame, pts), 136);
        assert_eq!(offset_of!(AVFrame, buf), 184);
        assert_eq!(offset_of!(AVFrame, color_range), 280);
        assert_eq!(offset_of!(AVFrame, colorspace), 292);
    }

    #[test]
    fn avformat_context_prefix_reaches_the_stream_array() {
        assert_eq!(offset_of!(AVFormatContext, nb_streams), 44);
        assert_eq!(offset_of!(AVFormatContext, streams), 48);
        assert_eq!(offset_of!(AVFormatContext, duration), 104);
    }

    #[test]
    fn avstream_prefix_reaches_codecpar_and_time_base() {
        assert_eq!(offset_of!(AVStream, index), 8);
        assert_eq!(offset_of!(AVStream, codecpar), 16);
        assert_eq!(offset_of!(AVStream, time_base), 32);
        assert_eq!(offset_of!(AVStream, nb_frames), 56);
    }

    #[test]
    fn codec_parameters_prefix_reaches_colour_and_audio_fields() {
        assert_eq!(offset_of!(AVCodecParameters, codec_id), 4);
        assert_eq!(offset_of!(AVCodecParameters, width), 72);
        assert_eq!(offset_of!(AVCodecParameters, color_range), 100);
        assert_eq!(offset_of!(AVCodecParameters, color_space), 112);
        assert_eq!(offset_of!(AVCodecParameters, ch_layout), 128);
        assert_eq!(offset_of!(AVCodecParameters, sample_rate), 152);
    }

    #[test]
    fn the_pixel_format_descriptor_prefix_reaches_the_component_depths() {
        assert_eq!(offset_of!(AVPixFmtDescriptor, nb_components), 8);
        assert_eq!(offset_of!(AVPixFmtDescriptor, log2_chroma_w), 9);
        assert_eq!(offset_of!(AVPixFmtDescriptor, log2_chroma_h), 10);
        assert_eq!(offset_of!(AVPixFmtDescriptor, flags), 16);
        assert_eq!(offset_of!(AVPixFmtDescriptor, comp), 24);
        assert_eq!(size_of::<AVComponentDescriptor>(), 20);
        assert_eq!(offset_of!(AVComponentDescriptor, depth), 16);
    }

    #[test]
    fn channel_layout_is_the_size_c_expects() {
        assert_eq!(size_of::<AVChannelLayout>(), 24);
        assert_eq!(align_of::<AVChannelLayout>(), 8);
    }

    #[test]
    fn the_eof_sentinel_matches_ffmpegs_own_tag() {
        assert_eq!(AVERROR_EOF, -541_478_725);
    }

    #[test]
    fn rational_converts_and_survives_a_zero_denominator() {
        assert_eq!(AVRational { num: 1, den: 4 }.as_f64(), 0.25);
        assert_eq!(AVRational { num: 1, den: 0 }.as_f64(), 0.0);
    }
}
