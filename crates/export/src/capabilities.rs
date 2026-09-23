#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportCapabilities {
    pub h264: bool,
    pub aac: bool,
    pub stream_copy: bool,
}

impl ExportCapabilities {
    pub fn probe() -> Self {
        Self {
            h264: h264_available(),
            aac: aac_available(),
            stream_copy: stream_copy_available(),
        }
    }

    pub fn audio_strategy(self) -> AudioStrategy {
        if self.aac {
            AudioStrategy::Aac
        } else {
            AudioStrategy::WavSidecar
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioStrategy {
    Aac,
    WavSidecar,
    StreamCopy,
}

#[cfg(not(target_arch = "wasm32"))]
fn h264_available() -> bool {
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
