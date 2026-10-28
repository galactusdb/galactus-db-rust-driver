// Records the compiler version for the Bolt `bolt_agent` (`Rust/<version>`).
// Standard library only; if rustc cannot be queried the agent says "Rust".
use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=RUSTC");
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let Ok(out) = Command::new(rustc).arg("--version").output() else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let Ok(text) = String::from_utf8(out.stdout) else {
        return;
    };
    let text = text.trim();
    if let Some(version) = text.split_whitespace().nth(1) {
        println!("cargo:rustc-env=GALACTUS_RUSTC_VERSION={version}");
        println!("cargo:rustc-env=GALACTUS_RUSTC_DETAILS={text}");
    }
}
