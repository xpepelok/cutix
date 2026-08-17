use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let flags = manifest.join("assets/flags");
    println!("cargo:rerun-if-changed={}", flags.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("assets/countries.json").display()
    );

    let mut names: Vec<String> = std::fs::read_dir(&flags)
        .expect("assets/flags")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("svg") {
                return None;
            }
            Some(path.file_stem()?.to_str()?.to_owned())
        })
        .collect();
    names.sort();

    let mut generated = String::from("pub const FLAG_SVGS: &[(&str, &[u8])] = &[\n");
    for name in &names {
        writeln!(
            generated,
            "    (\"{upper}\", include_bytes!(\"{dir}/{name}.svg\").as_slice()),",
            upper = name.to_uppercase(),
            dir = flags.display().to_string().replace('\\', "/"),
        )
        .expect("write");
    }
    generated.push_str("];\n");

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("flags.rs");
    std::fs::write(out, generated).expect("write flags.rs");
}
