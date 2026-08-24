const WINDOW_SECONDS: f32 = 0.02;
const SILENCE_FLOOR_DB: f32 = -100.0;

pub const DEFAULT_SILENCE_THRESHOLD_DB: f32 = -40.0;
pub const DEFAULT_MIN_SILENCE_SECONDS: f32 = 0.5;
pub const DEFAULT_SILENCE_PADDING_SECONDS: f32 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SilentRange {
    pub start_seconds: f64,
    pub end_seconds: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct SilenceOptions {
    pub sample_rate: u32,
    pub threshold_db: f32,
    pub min_silence_seconds: f32,
    pub padding_seconds: f32,
}

impl Default for SilenceOptions {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            threshold_db: DEFAULT_SILENCE_THRESHOLD_DB,
            min_silence_seconds: DEFAULT_MIN_SILENCE_SECONDS,
            padding_seconds: DEFAULT_SILENCE_PADDING_SECONDS,
        }
    }
}

pub fn detect_silent_ranges(samples: &[f32], options: &SilenceOptions) -> Vec<SilentRange> {
    if samples.is_empty() || options.sample_rate == 0 {
        return Vec::new();
    }

    let sample_rate = options.sample_rate as f64;
    let window_size = ((WINDOW_SECONDS * options.sample_rate as f32).round() as usize).max(1);
    let window_count = samples.len().div_ceil(window_size);
    let total_seconds = samples.len() as f64 / sample_rate;
    let padding = options.padding_seconds as f64;
    let minimum = options.min_silence_seconds as f64;

    let mut ranges: Vec<SilentRange> = Vec::new();
    let mut run_start: Option<usize> = None;

    let close_run =
        |run_start: &mut Option<usize>, end_window: usize, ranges: &mut Vec<SilentRange>| {
            let Some(start_window) = run_start.take() else {
                return;
            };
            let start_seconds = (start_window * window_size) as f64 / sample_rate;
            let end_seconds = total_seconds.min((end_window * window_size) as f64 / sample_rate);
            if end_seconds - start_seconds < minimum {
                return;
            }
            let padded_start = start_seconds + padding;
            let padded_end = end_seconds - padding;
            if padded_end - padded_start <= 0.0 {
                return;
            }
            ranges.push(SilentRange {
                start_seconds: padded_start,
                end_seconds: padded_end,
            });
        };

    for window_index in 0..window_count {
        let start = window_index * window_size;
        let end = samples.len().min(start + window_size);
        let sum: f64 = samples[start..end]
            .iter()
            .map(|value| (*value as f64) * (*value as f64))
            .sum();
        let rms = (sum / (end - start).max(1) as f64).sqrt();
        let decibels = if rms > 0.0 {
            20.0 * rms.log10()
        } else {
            SILENCE_FLOOR_DB as f64
        };

        if decibels < options.threshold_db as f64 {
            if run_start.is_none() {
                run_start = Some(window_index);
            }
            continue;
        }
        close_run(&mut run_start, window_index, &mut ranges);
    }

    close_run(&mut run_start, window_count, &mut ranges);

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal(sample_rate: u32, plan: &[(bool, f64)]) -> Vec<f32> {
        let total: f64 = plan.iter().map(|(_, duration)| *duration).sum();
        let mut samples = vec![0.0f32; (total * sample_rate as f64).round() as usize];
        let mut index = 0usize;
        for (is_tone, duration) in plan {
            let count = (*duration * sample_rate as f64).round() as usize;
            for _ in 0..count {
                if index >= samples.len() {
                    break;
                }
                if *is_tone {
                    let phase = std::f64::consts::TAU * 440.0 * index as f64 / sample_rate as f64;
                    samples[index] = (phase.sin() * 0.5) as f32;
                }
                index += 1;
            }
        }
        samples
    }

    fn rounded(ranges: &[SilentRange]) -> Vec<(f64, f64)> {
        ranges
            .iter()
            .map(|range| {
                (
                    (range.start_seconds * 1000.0).round() / 1000.0,
                    (range.end_seconds * 1000.0).round() / 1000.0,
                )
            })
            .collect()
    }

    #[test]
    fn finds_the_silent_gaps_of_a_tone_silence_tone_signal() {
        let samples = signal(
            48_000,
            &[
                (true, 1.0),
                (false, 1.0),
                (true, 2.0),
                (false, 1.5),
                (true, 1.0),
            ],
        );
        let ranges = detect_silent_ranges(
            &samples,
            &SilenceOptions {
                sample_rate: 48_000,
                ..SilenceOptions::default()
            },
        );
        assert_eq!(rounded(&ranges), vec![(1.05, 1.95), (4.05, 5.45)]);
    }

    #[test]
    fn a_gap_shorter_than_the_minimum_is_ignored() {
        let samples = signal(48_000, &[(true, 1.0), (false, 0.3), (true, 1.0)]);
        let ranges = detect_silent_ranges(&samples, &SilenceOptions::default());
        assert!(ranges.is_empty(), "unexpected ranges: {ranges:?}");
    }

    #[test]
    fn a_fully_silent_buffer_is_one_range_minus_padding() {
        let samples = vec![0.0f32; 48_000 * 2];
        let ranges = detect_silent_ranges(&samples, &SilenceOptions::default());
        assert_eq!(rounded(&ranges), vec![(0.05, 1.95)]);
    }

    #[test]
    fn a_loud_buffer_has_no_silence() {
        let samples = signal(48_000, &[(true, 2.0)]);
        assert!(detect_silent_ranges(&samples, &SilenceOptions::default()).is_empty());
    }

    #[test]
    fn a_lower_threshold_finds_less_silence() {
        let mut samples = signal(48_000, &[(true, 1.0), (false, 1.0), (true, 1.0)]);
        for sample in &mut samples[48_000..96_000] {
            *sample = 0.005;
        }
        let quiet = SilenceOptions {
            threshold_db: -40.0,
            ..SilenceOptions::default()
        };
        let strict = SilenceOptions {
            threshold_db: -50.0,
            ..SilenceOptions::default()
        };
        assert_eq!(detect_silent_ranges(&samples, &quiet).len(), 1);
        assert!(detect_silent_ranges(&samples, &strict).is_empty());
    }

    #[test]
    fn padding_shrinks_each_range_from_both_ends() {
        let samples = signal(48_000, &[(true, 1.0), (false, 1.0), (true, 1.0)]);
        let none = SilenceOptions {
            padding_seconds: 0.0,
            ..SilenceOptions::default()
        };
        assert_eq!(
            rounded(&detect_silent_ranges(&samples, &none)),
            vec![(1.0, 2.0)]
        );
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(detect_silent_ranges(&[], &SilenceOptions::default()).is_empty());
    }
}
