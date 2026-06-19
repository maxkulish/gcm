// Stamps the build with the current git short SHA so `gcm --version` reports a
// build-stamped string (AC-1). Falls back to "unknown" outside a git checkout.
use std::process::Command;

fn main() {
    let sha = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GCM_GIT_SHA={sha}");
    // Rerun when HEAD moves: .git/HEAD changes on branch switch; .git/logs/HEAD
    // is appended on every commit/checkout/reset, catching new commits on the
    // same branch too.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/logs/HEAD");
}
