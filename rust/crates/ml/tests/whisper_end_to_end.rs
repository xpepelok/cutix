use std::path::Path;
use std::time::Instant;

use ml::whisper::{
    ensure_whisper_downloaded, find_whisper, transcribe, whisper_cached_size_bytes,
    whisper_directory, WhisperProgress,
};

fn decode_mp3(path: &Path) -> (Vec<f32>, u32) {
    let bytes = std::fs::read(path).expect("read mp3");
    let (header, samples) = puremp3::read_mp3(&bytes[..]).expect("decode mp3");
    let mono: Vec<f32> = samples.map(|(left, right)| (left + right) * 0.5).collect();
    (mono, header.sample_rate.hz())
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|character: char| !character.is_alphanumeric() && character != '\'')
        .filter(|word| !word.is_empty())
        .map(|word| word.trim_matches('\'').to_string())
        .filter(|word| !word.is_empty())
        .collect()
}

fn reference_words(srt: &Path, start: f64, end: f64) -> Vec<String> {
    let input = std::fs::read_to_string(srt).expect("read srt");
    let parsed = cutix_project::parse_srt(&input);
    let mut collected = Vec::new();
    for cue in &parsed.captions {
        if cue.end_time() > start && cue.start_time < end {
            collected.extend(words(&cue.text));
        }
    }
    collected
}

#[test]
#[ignore]
fn transcribes_known_dialogue_and_matches_the_official_subtitles() {
    let audio = fixtures::fixture_or_skip!(fixtures::AUDIO_CLIP);
    let srt = fixtures::fixture_or_skip!(fixtures::SUBTITLES_EN);
    let spec = find_whisper("whisper-tiny").expect("model");

    let started = Instant::now();
    let mut last = String::new();
    ensure_whisper_downloaded(
        spec,
        &mut |progress| {
            if let WhisperProgress::Downloading { file, done } = progress {
                if file != last {
                    println!("downloading {file}");
                    last = file;
                }
                let _ = done;
            }
        },
        &|| false,
    )
    .expect("download");
    println!("download wall time: {:?}", started.elapsed());
    for entry in std::fs::read_dir(whisper_directory(spec)).expect("cache dir") {
        let entry = entry.expect("entry");
        println!(
            "  {} = {} bytes",
            entry.file_name().to_string_lossy(),
            entry.metadata().expect("metadata").len()
        );
    }
    println!("total cached: {} bytes", whisper_cached_size_bytes(spec));

    let (samples, sample_rate) = decode_mp3(&audio);
    let start = (fixtures::AUDIO_DIALOGUE_START * sample_rate as f64) as usize;
    let end = ((fixtures::AUDIO_DIALOGUE_END * sample_rate as f64) as usize).min(samples.len());
    let window = &samples[start.min(end)..end];
    println!(
        "audio: {:.2}s at {sample_rate} Hz, {} samples",
        window.len() as f64 / sample_rate as f64,
        window.len()
    );

    let started = Instant::now();
    let transcript = transcribe(
        window,
        sample_rate,
        "whisper-tiny",
        Some("en"),
        &mut |_| {},
        &|| false,
    )
    .expect("transcribe");
    println!("transcription wall time: {:?}", started.elapsed());
    println!("language: {}", transcript.language);
    for segment in &transcript.segments {
        println!(
            "[{:>7.2} -> {:>7.2}] {}",
            segment.start, segment.end, segment.text
        );
    }
    assert!(!transcript.segments.is_empty());

    let heard: Vec<String> = transcript
        .segments
        .iter()
        .flat_map(|segment| words(&segment.text))
        .collect();
    let expected = reference_words(
        &srt,
        fixtures::AUDIO_DIALOGUE_START,
        fixtures::AUDIO_DIALOGUE_END,
    );
    assert!(
        expected.len() > 30,
        "the subtitle window carries too little text to be an oracle: {expected:?}"
    );

    let distinctive: Vec<&String> = expected.iter().filter(|word| word.len() >= 5).collect();
    let recalled = distinctive
        .iter()
        .filter(|word| heard.contains(word))
        .count();
    let recall = recalled as f64 / distinctive.len() as f64;
    println!(
        "recall {recall:.2} ({recalled}/{}) against the official subtitles",
        distinctive.len()
    );
    println!("expected: {}", expected.join(" "));
    println!("heard:    {}", heard.join(" "));
    assert!(
        recall >= 0.5,
        "whisper-tiny recovered only {recall:.2} of the distinctive subtitle words"
    );
}

#[test]
#[ignore]
fn transcribes_synthesised_speech() {
    let text = "The quick brown fox jumps over the lazy dog.";
    let (samples, sample_rate) =
        speech::synthesize_with(text, speech::models::DEFAULT_MODEL, "af_heart", |_, _| {})
            .expect("synthesize");
    let transcript = transcribe(
        &samples,
        sample_rate,
        "whisper-tiny",
        Some("en"),
        &mut |_| {},
        &|| false,
    )
    .expect("transcribe");
    println!("language: {}", transcript.language);
    for segment in &transcript.segments {
        println!(
            "[{:>7.2} -> {:>7.2}] {}",
            segment.start, segment.end, segment.text
        );
    }
    assert!(!transcript.segments.is_empty());
}
