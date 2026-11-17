use std::{
    env, fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn output(program: &str, args: &[&str]) -> Option<String> {
    let result = Command::new(program).args(args).output().ok()?;
    result
        .status
        .success()
        .then(|| String::from_utf8_lossy(&result.stdout).trim().to_owned())
}

fn emit(name: &str, value: &str) {
    println!(
        "cargo:rustc-env=WV_{name}={}",
        value.replace('\r', "").replace('\n', "; ")
    );
}

fn main() {
    for path in [
        "build.rs",
        "Cargo.toml",
        "Cargo.lock",
        "src",
        "tests",
        "README.md",
        ".gitignore",
        ".agents/notes",
        ".scratch/v1-exe",
        "docs",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    for file in ["HEAD", "index", "packed-refs"] {
        if let Some(path) = output("git", &["rev-parse", "--git-path", file]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(reference) = output("git", &["symbolic-ref", "-q", "HEAD"]) {
        if let Some(path) = output("git", &["rev-parse", "--git-path", &reference]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    let rustc = env::var("RUSTC").expect("Cargo must provide RUSTC");
    let cargo = env::var("CARGO").expect("Cargo must provide CARGO");
    emit(
        "RUSTC",
        &output(&rustc, &["-Vv"]).expect("could not read rustc version"),
    );
    emit(
        "CARGO",
        &output(&cargo, &["-V"]).expect("could not read cargo version"),
    );
    emit(
        "TARGET",
        &env::var("TARGET").expect("Cargo must provide TARGET"),
    );
    emit(
        "PROFILE",
        &env::var("PROFILE").expect("Cargo must provide PROFILE"),
    );
    emit(
        "REVISION",
        &output("git", &["rev-parse", "HEAD"])
            .unwrap_or_else(|| "unknown (no git revision)".to_owned()),
    );
    let dirty = output("git", &["status", "--porcelain"])
        .map(|status| (!status.is_empty()).to_string())
        .unwrap_or_else(|| "unknown (git status unavailable)".to_owned());
    emit("DIRTY", &dirty);
    emit(
        "BUILD_UNIX_MS",
        &SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before Unix epoch")
            .as_millis()
            .to_string(),
    );

    // Read only Cargo-generated name/version pairs; this is not a general TOML parser.
    let lock = fs::read_to_string("Cargo.lock")
        .expect("Cargo.lock is required for traceable validation builds");
    let wanted = [
        "eframe",
        "egui",
        "egui-winit",
        "egui_glow",
        "egui_system_font_provider",
        "epaint",
        "winit",
        "raw-window-handle",
        "windows-sys",
        "glutin",
        "glutin-winit",
        "glow",
    ];
    let mut name = "";
    let mut versions = Vec::new();
    for line in lock.lines() {
        if line == "[[package]]" {
            name = "";
        }
        if let Some(value) = line
            .strip_prefix("name = \"")
            .and_then(|value| value.strip_suffix('"'))
        {
            name = value;
        }
        if let Some(version) = line
            .strip_prefix("version = \"")
            .and_then(|value| value.strip_suffix('"'))
        {
            if wanted.contains(&name) {
                versions.push(format!("{name}={version}"));
            }
        }
    }
    versions.sort();
    assert!(
        versions.iter().any(|value| value == "eframe=0.37.0"),
        "missing pinned eframe in Cargo.lock"
    );
    emit("DEPENDENCIES", &versions.join(", "));
}
