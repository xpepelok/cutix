use std::io::Write;
use std::path::Path;

use cutix_playback::AudioBuffer;

use crate::error::{ExportError, Result};

/// Bytes in the canonical 16-bit PCM header written in front of the samples.
const HEADER_BYTES: usize = 44;

pub fn write_wav(path: &Path, audio: &AudioBuffer) -> Result<u64> {
    let header =
        header(audio.channels, audio.sample_rate, audio.interleaved.len()).ok_or_else(|| {
            ExportError::Io {
                path: path.display().to_string(),
                detail: "the audio is too long for a WAV file (its sizes are 32-bit)".to_string(),
            }
        })?;

    let mut bytes = Vec::with_capacity(HEADER_BYTES + audio.interleaved.len() * 2);
    bytes.extend_from_slice(&header);
    for sample in &audio.interleaved {
        let clamped = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        bytes.extend_from_slice(&clamped.to_le_bytes());
    }

    let mut file = std::fs::File::create(path).map_err(|error| ExportError::Io {
        path: path.display().to_string(),
        detail: error.to_string(),
    })?;
    file.write_all(&bytes).map_err(|error| ExportError::Io {
        path: path.display().to_string(),
        detail: error.to_string(),
    })?;
    Ok(bytes.len() as u64)
}

/// The RIFF header for `samples` interleaved 16-bit samples, or `None` when a size field
/// would not fit.
///
/// RIFF stores every size as a `u32`, so a mix of more than about 4 GiB (roughly 6.2 hours
/// of 48 kHz stereo) cannot be described. Casting would silently wrap the sizes and write a
/// file that players read as a few seconds long, so the caller gets an error instead.
fn header(channels: usize, sample_rate: u32, samples: usize) -> Option<[u8; HEADER_BYTES]> {
    let channels = u16::try_from(channels.max(1)).ok()?;
    let sample_rate = sample_rate.max(1);
    let bits = 16u16;
    let block_align = channels.checked_mul(bits / 8)?;
    let byte_rate = sample_rate.checked_mul(u32::from(block_align))?;
    let data_bytes = u32::try_from(samples.checked_mul(2)?).ok()?;
    let riff_bytes = data_bytes.checked_add(HEADER_BYTES as u32 - 8)?;

    let mut header = [0u8; HEADER_BYTES];
    let fields: [&[u8]; 12] = [
        b"RIFF",
        &riff_bytes.to_le_bytes(),
        b"WAVEfmt ",
        &16u32.to_le_bytes(),
        &1u16.to_le_bytes(),
        &channels.to_le_bytes(),
        &sample_rate.to_le_bytes(),
        &byte_rate.to_le_bytes(),
        &block_align.to_le_bytes(),
        &bits.to_le_bytes(),
        b"data",
        &data_bytes.to_le_bytes(),
    ];
    let mut offset = 0;
    for field in fields {
        header[offset..offset + field.len()].copy_from_slice(field);
        offset += field.len();
    }
    debug_assert_eq!(offset, HEADER_BYTES);
    Some(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_describes_the_samples_that_follow() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("mix.wav");
        let audio = AudioBuffer {
            sample_rate: 48_000,
            channels: 2,
            interleaved: vec![0.0, 1.0, -1.0, 0.5],
        };
        let written = write_wav(&path, &audio).expect("write");
        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(written as usize, bytes.len());
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            48_000
        );
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 8);
        assert_eq!(i16::from_le_bytes(bytes[46..48].try_into().unwrap()), 32767);
        assert_eq!(
            i16::from_le_bytes(bytes[48..50].try_into().unwrap()),
            -32767
        );
    }

    #[test]
    fn audio_too_long_for_32_bit_sizes_is_refused_rather_than_wrapped() {
        // 2^31 samples is 4 GiB of 16-bit data: one sample more than the data size field
        // can hold once the rest of the RIFF chunk is counted.
        assert!(header(2, 48_000, (u32::MAX as usize - 36) / 2).is_some());
        assert!(header(2, 48_000, 1 << 31).is_none());
        assert!(header(70_000, 48_000, 8).is_none());
    }
}
