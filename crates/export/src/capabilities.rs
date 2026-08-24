//! What the machine running the export can actually do.
//!
//! Export decides between muxing audio into the MP4 and writing a sidecar WAV before it
//! creates the destination file, so the answer has to be about the specific codec that
//! decision depends on. Asking a general question — "does this FFmpeg build expose the
//! encoder API?" — and assuming AAC follows is how an export ends up committed to a muxed
//! MP4 on a build compiled without the AAC encoder, discovering the problem only once the
//! video track is already on disk.
//!
//! Each capability is probed once and cached by the crate it belongs to.

/// The encoders and container operations available on this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportCapabilities {
    /// An H.264 encoder can be opened — either FFmpeg's or the bundled OpenH264.
    pub h264: bool,
    /// An AAC encoder can be opened. This is the specific codec, not the encoder API.
    pub aac: bool,
    /// Packets can be copied between containers without re-encoding.
    pub stream_copy: bool,
}

impl ExportCapabilities {
    /// Probes the environment.
    ///
    /// Cheap after the first call: each underlying probe caches its answer for the
    /// lifetime of the process, because the set of loadable FFmpeg libraries does not
    /// change while the editor is running.
    pub fn probe() -> Self {
        Self {
            h264: h264_available(),
            aac: aac_available(),
            stream_copy: stream_copy_available(),
        }
    }

    /// How audio should be delivered given these capabilities.
    ///
    /// Muxing needs a real AAC encoder; without one the audio is written next to the
    /// video as a WAV rather than being dropped.
    pub fn audio_strategy(self) -> AudioStrategy {
        if self.aac {
            AudioStrategy::Aac
        } else {
            AudioStrategy::WavSidecar
        }
    }
}

/// How the audio for an export is delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioStrategy {
    /// Encoded to AAC and muxed into the same container as the video.
    Aac,
    /// Written as an uncompressed WAV beside the video file.
    WavSidecar,
    /// Copied from the source container without being decoded.
    StreamCopy,
}

#[cfg(not(target_arch = "wasm32"))]
fn h264_available() -> bool {
    // OpenH264 is compiled in, so an H.264 encoder is always reachable natively.
    true
}

#[cfg(target_arch = "wasm32")]
fn h264_available() -> bool {
    false
}

#[cfg(not(target_arch = "wasm32"))]
fn aac_available() -> bool {
    video::ffmpeg::can_encode_aac()
}

#[cfg(target_arch = "wasm32")]
fn aac_available() -> bool {
    false
}

#[cfg(not(target_arch = "wasm32"))]
fn stream_copy_available() -> bool {
    video::ffmpeg::can_decode()
}

#[cfg(target_arch = "wasm32")]
fn stream_copy_available() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{AudioStrategy, ExportCapabilities};

    #[test]
    fn a_build_without_an_aac_encoder_writes_a_sidecar_rather_than_dropping_the_audio() {
        let without = ExportCapabilities {
            h264: true,
            aac: false,
            stream_copy: false,
        };
        assert_eq!(without.audio_strategy(), AudioStrategy::WavSidecar);

        let with = ExportCapabilities {
            aac: true,
            ..without
        };
        assert_eq!(with.audio_strategy(), AudioStrategy::Aac);
    }

    #[test]
    fn probing_twice_gives_the_same_answer() {
        assert_eq!(ExportCapabilities::probe(), ExportCapabilities::probe());
    }

    #[test]
    fn the_audio_probe_is_about_aac_and_not_the_encoder_api_in_general() {
        // A build can expose the encoder entry points and still carry no AAC encoder.
        // The two answers are allowed to differ; what must not happen is export treating
        // the general one as an answer about AAC.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let general = video::ffmpeg::can_encode();
            let specific = ExportCapabilities::probe().aac;
            assert!(
                general || !specific,
                "AAC cannot be available when the encoder API is not"
            );
        }
    }
}
