use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let templates = manifest.join("../../templates");
    let Ok(templates) = templates.canonicalize() else {
        let out = Path::new(&env::var("OUT_DIR").unwrap()).join("builtin.rs");
        fs::write(&out, "pub const BUILTIN: &[(&str, &str)] = &[];\n").unwrap();
        return;
    };

    println!("cargo:rerun-if-changed={}", templates.display());

    let mut files: Vec<PathBuf> = fs::read_dir(&templates)
        .expect("cannot read templates directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    files.sort();

    let mut entries = String::new();
    for file in &files {
        println!("cargo:rerun-if-changed={}", file.display());
        let name = file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        entries.push_str(&format!(
            "    (\"{name}\", include_str!(r\"{}\")),\n",
            file.display()
        ));
    }

    let generated = format!("pub const BUILTIN: &[(&str, &str)] = &[\n{entries}];\n");
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("builtin.rs");
    fs::write(out, generated).expect("cannot write generated templates");
}
