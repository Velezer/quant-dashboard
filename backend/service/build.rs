use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.file_type()?.is_dir() {
            if matches!(
                name.as_ref(),
                ".git" | ".agents" | ".serena" | "target" | "scratch" | "data" | "node_modules"
            ) {
                continue;
            }
            collect_files(&path, files)?;
        } else if entry.file_type()?.is_file()
            && !name.starts_with(".env")
            && !name.ends_with(".log")
            && !name.ends_with(".tmp")
        {
            files.push(path);
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../..").canonicalize()?;
    let mut files = Vec::new();
    for path in ["backend", "quant", "frontend", "docs"] {
        collect_files(&root.join(path), &mut files)?;
    }
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "Dockerfile",
        ".dockerignore",
        "railway.toml",
        "README.md",
        "ARCHITECTURE.md",
        "AGENTS.md",
    ] {
        let file = root.join(path);
        if file.is_file() {
            println!("cargo:rerun-if-changed={}", file.display());
            files.push(file);
        }
    }
    files.sort();

    // A deterministic source fingerprint works when the deployment builder
    // supplies an archive without the repository's .git directory.
    let mut fingerprint = 14_695_981_039_346_656_037_u64;
    for file in files {
        let relative = file.strip_prefix(&root).unwrap_or(&file);
        for byte in relative.to_string_lossy().bytes().chain([0]) {
            fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(1_099_511_628_211);
        }
        for byte in fs::read(file)? {
            fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(1_099_511_628_211);
        }
    }
    println!("cargo:rustc-env=QUANT_BUILD_ID={fingerprint:016x}");
    Ok(())
}
