use std::cell::RefCell;

use cutix_playback::output::AudioOutput;

const GAIN: f32 = 0.05;

const BACKLOG_LIMIT: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Click,
    Toggle,
    Done,
    Remove,
    Trouble,
}

impl Cue {
    fn voice(self) -> &'static [(f32, f32, f32)] {
        match self {
            Cue::Click => &[(440.0, 0.0, 0.055)],
            Cue::Toggle => &[(392.0, 0.0, 0.050), (523.25, 0.028, 0.065)],
            Cue::Done => &[
                (392.0, 0.0, 0.100),
                (523.25, 0.075, 0.120),
                (659.25, 0.155, 0.170),
            ],
            Cue::Remove => &[(294.0, 0.0, 0.090), (196.0, 0.060, 0.130)],
            Cue::Trouble => &[(233.0, 0.0, 0.140), (175.0, 0.095, 0.180)],
        }
    }
}

fn envelope(position: f32, length: f32) -> f32 {
    if position < 0.0 || position > length || length <= 0.0 {
        return 0.0;
    }
    let attack = (length * 0.30).max(0.004);
    let rise = (position / attack).min(1.0);
    let decay = (-3.2 * position / length).exp();
    rise * decay
}

pub fn render(cue: Cue, sample_rate: u32) -> Vec<f32> {
    let rate = sample_rate.max(1) as f32;
    let span = cue
        .voice()
        .iter()
        .map(|(_, start, length)| start + length)
        .fold(0.0f32, f32::max);
    let frames = (span * rate).ceil() as usize;
    let mut mono = vec![0.0f32; frames];

    for (frequency, start, length) in cue.voice() {
        let from = (start * rate) as usize;
        let count = (length * rate) as usize;
        for index in 0..count {
            let slot = from + index;
            if slot >= mono.len() {
                break;
            }
            let position = index as f32 / rate;
            let phase = std::f32::consts::TAU * frequency * position;
            mono[slot] += phase.sin() * envelope(position, *length);
        }
    }

    let peak = mono
        .iter()
        .fold(0.0f32, |top, sample| top.max(sample.abs()));
    if peak > 0.0 {
        let scale = GAIN / peak;
        for sample in mono.iter_mut() {
            *sample *= scale;
        }
    }
    mono
}

pub fn interleave(mono: &[f32], channels: usize) -> Vec<f32> {
    let channels = channels.max(1);
    let mut out = Vec::with_capacity(mono.len() * channels);
    for sample in mono {
        for _ in 0..channels {
            out.push(*sample);
        }
    }
    out
}

struct Player {
    output: Option<AudioOutput>,
    muted: bool,
}

impl Player {
    fn play(&mut self, cue: Cue) {
        if self.muted {
            return;
        }
        if self.output.is_none() {
            self.output = AudioOutput::open().ok();
        }
        let Some(output) = self.output.as_ref() else {
            return;
        };

        let channels = output.channels().max(1);
        let voice = render(cue, output.sample_rate());
        if output.queued_samples() > voice.len() * channels * BACKLOG_LIMIT {
            return;
        }
        output.set_volume(1.0);
        output.queue_samples(&interleave(&voice, channels));
        output.start();
    }
}

thread_local! {
    static PLAYER: RefCell<Player> = const {
        RefCell::new(Player {
            output: None,
            muted: false,
        })
    };
}

/// Plays a short interface sound.
///
/// Does nothing under test: a cue is feedback for someone watching the screen, and opening
/// the output device to produce one means initialising the platform audio stack from
/// whatever thread the harness happens to be on. `render` and `interleave` are what the
/// tests here exercise instead.
///
/// Never blocks the caller waiting for the player: a cue that cannot be played is skipped,
/// because a sound is not worth stalling the interface for.
pub fn play(cue: Cue) {
    if cfg!(test) {
        return;
    }
    PLAYER.with(|player| {
        if let Ok(mut player) = player.try_borrow_mut() {
            player.play(cue);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A nominal device rate to render cues at. The tests that care about rate
    /// scaling name their own rates instead.
    const SAMPLE_RATE: u32 = 48_000;

    #[test]
    fn every_cue_makes_a_short_audible_sound() {
        for cue in [
            Cue::Click,
            Cue::Toggle,
            Cue::Done,
            Cue::Remove,
            Cue::Trouble,
        ] {
            let voice = render(cue, SAMPLE_RATE);
            assert!(!voice.is_empty(), "{cue:?} rendered nothing");
            let seconds = voice.len() as f32 / SAMPLE_RATE as f32;
            assert!(
                seconds > 0.02 && seconds < 0.4,
                "{cue:?} lasts {seconds}s, which is not a cue"
            );
            let peak = voice.iter().fold(0.0f32, |top, s| top.max(s.abs()));
            assert!((peak - GAIN).abs() < 0.01, "{cue:?} peaks at {peak}");
        }
    }

    #[test]
    fn a_cue_opens_quietly_and_dies_away_rather_than_clicking_at_both_ends() {
        let voice = render(Cue::Click, SAMPLE_RATE);
        assert!(voice[0].abs() < 0.01, "the cue starts with a step");
        let tail = voice[voice.len() - 1].abs();
        let middle = voice[voice.len() / 4].abs();
        assert!(tail < middle * 0.5, "the cue is cut off instead of fading");
    }

    #[test]
    fn the_cues_are_told_apart_by_their_length() {
        let click = render(Cue::Click, SAMPLE_RATE).len();
        let done = render(Cue::Done, SAMPLE_RATE).len();
        let remove = render(Cue::Remove, SAMPLE_RATE).len();
        assert!(click < remove, "a click must be the shortest of them");
        assert!(remove < done, "finishing a job deserves the longest cue");
    }

    #[test]
    fn interleaving_hands_every_speaker_the_same_sample() {
        let mono = vec![0.5, -0.25];
        assert_eq!(interleave(&mono, 2), vec![0.5, 0.5, -0.25, -0.25]);
        assert_eq!(interleave(&mono, 1), mono);
        assert_eq!(interleave(&[], 2), Vec::<f32>::new());
    }

    #[test]
    fn a_cue_scales_with_the_device_rate() {
        let slow = render(Cue::Click, 24_000).len();
        let fast = render(Cue::Click, 48_000).len();
        assert!(
            (fast as f32 / slow as f32 - 2.0).abs() < 0.05,
            "{slow} at 24k against {fast} at 48k"
        );
    }
}
