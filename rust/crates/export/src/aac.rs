use cutix_playback::AudioBuffer;

use crate::error::{ExportError, Result};

pub const AAC_FRAME_SAMPLES: u32 = 1024;

const BITRATE_PER_CHANNEL_BPS: u32 = 96_000;

const SAMPLING_FREQUENCIES: [u32; 13] = [
    96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000,
    7_350,
];

#[derive(Clone, Debug)]
pub struct AacTrack {
    pub sample_rate: u32,
    pub channels: u16,
    pub packets: Vec<Vec<u8>>,
}

impl AacTrack {
    pub fn sample_count(&self) -> u64 {
        self.packets.len() as u64 * AAC_FRAME_SAMPLES as u64
    }

    pub fn byte_count(&self) -> u64 {
        self.packets.iter().map(|packet| packet.len() as u64).sum()
    }
}

pub fn sf_index_for_rate(sample_rate: u32) -> Option<u8> {
    SAMPLING_FREQUENCIES
        .iter()
        .position(|rate| *rate == sample_rate)
        .map(|index| index as u8)
}

pub fn bitrate_for(channels: usize) -> u32 {
    (channels.clamp(1, 6) as u32) * BITRATE_PER_CHANNEL_BPS
}

#[cfg(not(target_arch = "wasm32"))]
pub fn is_available() -> bool {
    video::ffmpeg::can_encode()
}

#[cfg(target_arch = "wasm32")]
pub fn is_available() -> bool {
    false
}

pub fn unavailable_reason() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        video::ffmpeg::status()
    }
    #[cfg(target_arch = "wasm32")]
    {
        "aac encoding is native-only".to_owned()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn encode(audio: &AudioBuffer) -> Result<AacTrack> {
    let channels = audio.channels.max(1);
    let sample_rate = audio.sample_rate;
    if sf_index_for_rate(sample_rate).is_none() {
        return Err(ExportError::Encoder(format!(
            "{sample_rate} Hz has no AAC sampling-frequency index"
        )));
    }
    if audio.interleaved.is_empty() {
        return Err(ExportError::Encoder("the mixdown is empty".to_owned()));
    }

    let packets = video::ffmpeg::encode_aac(
        &audio.interleaved,
        channels,
        sample_rate,
        bitrate_for(channels),
    )
    .map_err(ExportError::Encoder)?;

    if packets.is_empty() {
        return Err(ExportError::Encoder(
            "the encoder produced no access units".to_owned(),
        ));
    }

    Ok(AacTrack {
        sample_rate,
        channels: channels as u16,
        packets,
    })
}

#[cfg(target_arch = "wasm32")]
pub fn encode(_audio: &AudioBuffer) -> Result<AacTrack> {
    Err(ExportError::Encoder(unavailable_reason()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_bitrate_scales_with_the_channel_count() {
        assert_eq!(bitrate_for(1), 96_000);
        assert_eq!(bitrate_for(2), 192_000);
        assert_eq!(bitrate_for(9), 576_000);
    }

    #[test]
    fn the_sampling_frequency_table_matches_the_standard() {
        assert_eq!(sf_index_for_rate(96_000), Some(0));
        assert_eq!(sf_index_for_rate(48_000), Some(3));
        assert_eq!(sf_index_for_rate(44_100), Some(4));
        assert_eq!(sf_index_for_rate(8_000), Some(11));
        assert_eq!(sf_index_for_rate(7_350), Some(12));
    }

    #[test]
    fn a_rate_with_no_sampling_index_has_none() {
        assert_eq!(sf_index_for_rate(47_999), None);
        assert_eq!(sf_index_for_rate(0), None);
    }

    #[test]
    fn a_rate_with_no_sampling_index_is_refused_rather_than_mislabelled() {
        let audio = AudioBuffer {
            sample_rate: 47_999,
            channels: 2,
            interleaved: vec![0.0; 2048],
        };
        let error = encode(&audio).expect_err("refused");
        assert!(matches!(error, ExportError::Encoder(_)));
    }

    #[test]
    fn an_empty_mixdown_is_refused_before_the_encoder_is_opened() {
        let audio = AudioBuffer {
            sample_rate: 48_000,
            channels: 2,
            interleaved: Vec::new(),
        };
        assert!(matches!(
            encode(&audio).expect_err("refused"),
            ExportError::Encoder(_)
        ));
    }
}
