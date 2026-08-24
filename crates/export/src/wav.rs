use std::io::Write;
use std::path::Path;

use cutix_playback::AudioBuffer;

use crate::error::{ExportError, Result};

pub fn write_wav(path: &Path, audio: &AudioBuffer) -> Result<u64> {
    let channels = audio.channels.max(1) as u16;
    let sample_rate = audio.sample_rate.max(1);
    let bits = 16u16;
    let block_align = channels * bits / 8;
    let byte_rate = sample_rate * block_align as u32;
    let data_bytes = (audio.interleaved.len() * 2) as u32;

    let mut bytes = Vec::with_capacity(44 + data_bytes as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
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
}
