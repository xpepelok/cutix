use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let lang_dir = manifest.join("../../../lang");
    let lang_dir = lang_dir.canonicalize().expect("lang directory not found");

    println!("cargo:rerun-if-changed={}", lang_dir.display());

    let mut locales: Vec<String> = fs::read_dir(&lang_dir)
        .expect("cannot read lang directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .filter_map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
        })
        .filter(|locale| locale != "index")
        .collect();
    locales.sort();

    assert!(
        locales.iter().any(|locale| locale == "en"),
        "lang/en.json is required as the fallback locale"
    );

    for locale in &locales {
        println!(
            "cargo:rerun-if-changed={}",
            lang_dir.join(format!("{locale}.json")).display()
        );
    }

    let entries: String = locales
        .iter()
        .map(|locale| {
            format!(
                "    (\"{locale}\", include_str!(r\"{}\")),\n",
                lang_dir.join(format!("{locale}.json")).display()
            )
        })
        .collect();

    let generated = format!("pub const EMBEDDED: &[(&str, &str)] = &[\n{entries}];\n");

    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("locales.rs");
    fs::write(out, generated).expect("cannot write generated locales");
}
