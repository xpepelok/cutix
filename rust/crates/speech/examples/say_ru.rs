use std::env;

fn main() {
    let mut arguments = env::args().skip(1);
    let text = arguments
        .next()
        .unwrap_or_else(|| "Привет, это тест синтеза речи.".to_string());
    let voice_key = arguments
        .next()
        .unwrap_or_else(|| speech::models::DEFAULT_RUSSIAN_VOICE.to_string());
    let output = arguments.next();

    let Some(voice) = speech::models::find_voice(&voice_key) else {
        eprintln!("unknown voice {voice_key}");
        eprintln!("russian voices:");
        for candidate in speech::models::voices_for_language("ru-RU") {
            eprintln!(
                "  {} — {}",
                candidate.key,
                speech::models::describe_voice(candidate)
            );
        }
        std::process::exit(1);
    };

    println!("voice:    {}", speech::models::describe_voice(voice));
    println!("text:     {text}");
    println!("phonemes: {}", speech::ru::phonemize(&text));

    let mut last_reported = String::new();
    let result = speech::synthesize_with(
        &text,
        speech::models::DEFAULT_MODEL,
        &voice_key,
        |asset, done| {
            let step = format!("{asset} {:.0}%", done * 100.0);
            if step != last_reported {
                println!("downloading {step}");
                last_reported = step;
            }
        },
    );

    let (samples, sample_rate) = match result {
        Ok(audio) => audio,
        Err(error) => {
            eprintln!("synthesis failed: {error}");
            std::process::exit(1);
        }
    };

    let level = speech::rms(&samples);
    let peak = samples.iter().fold(0.0_f32, |best, s| best.max(s.abs()));
    println!("samples:  {}", samples.len());
    println!("rate:     {sample_rate} Hz");
    println!(
        "duration: {:.2} s",
        samples.len() as f32 / sample_rate as f32
    );
    println!("rms:      {level:.6}");
    println!("peak:     {peak:.6}");

    let window = (sample_rate as usize / 20).max(1);
    let envelope: Vec<f32> = samples.chunks(window).map(speech::rms).collect();
    let loudest = envelope.iter().cloned().fold(0.0_f32, f32::max).max(1e-9);
    println!("envelope (50 ms per column, relative to peak):");
    for (index, value) in envelope.iter().enumerate() {
        let bars = ((value / loudest) * 40.0).round() as usize;
        println!(
            "  {:>5.2}s |{:<40}| {:.4}",
            index as f32 * 0.05,
            "#".repeat(bars),
            value
        );
    }

    if let Some(path) = output {
        std::fs::write(&path, speech::to_wav(&samples, sample_rate)).expect("failed to write wav");
        println!("wrote {path}");
    }

    if level <= 0.0 {
        eprintln!("output is silent");
        std::process::exit(2);
    }
}
