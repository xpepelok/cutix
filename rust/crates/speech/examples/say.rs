use std::env;

fn main() {
    let mut arguments = env::args().skip(1);
    let text = arguments
        .next()
        .unwrap_or_else(|| "Hello from cutix. Native speech synthesis is running.".to_string());
    let voice = arguments
        .next()
        .unwrap_or_else(|| speech::models::DEFAULT_VOICE.to_string());
    let output = arguments.next();

    let model = speech::models::default_model();
    println!("model: {}", speech::models::describe_model(model));
    match speech::models::find_voice(&voice) {
        Some(spec) => println!("voice: {}", speech::models::describe_voice(spec)),
        None => {
            eprintln!("unknown voice {voice}");
            std::process::exit(1);
        }
    }
    println!("text:  {text}");

    let mut last_reported = String::new();
    let result = speech::synthesize_with(&text, model.key, &voice, |asset, done| {
        let step = format!("{asset} {:.0}%", done * 100.0);
        if step != last_reported {
            println!("downloading {step}");
            last_reported = step;
        }
    });

    match result {
        Ok((samples, sample_rate)) => {
            let level = speech::rms(&samples);
            let peak = samples.iter().fold(0.0_f32, |best, s| best.max(s.abs()));
            println!("samples: {}", samples.len());
            println!("sample rate: {sample_rate}");
            println!(
                "duration: {:.2} s",
                samples.len() as f32 / sample_rate as f32
            );
            println!("rms: {level:.6}");
            println!("peak: {peak:.6}");
            if let Some(path) = output {
                std::fs::write(&path, speech::to_wav(&samples, sample_rate))
                    .expect("failed to write wav");
                println!("wrote {path}");
            }
            if level <= 0.0 {
                eprintln!("output is silent");
                std::process::exit(2);
            }
        }
        Err(error) => {
            eprintln!("synthesis failed: {error}");
            std::process::exit(1);
        }
    }
}
