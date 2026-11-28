//! Embeds the exact source commit identity into every Hydra binary so trusted evidence and
//! restricted pilot authorizations can be bound to the precise executable build.
fn main() {
    let commit = std::env::var("HYDRA_BUILD_COMMIT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        })
        .filter(|v| v.len() == 40 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unknown".to_string());
    let dirty = std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);
    println!("cargo:rustc-env=HYDRA_BUILD_COMMIT={commit}");
    println!(
        "cargo:rustc-env=HYDRA_BUILD_DIRTY={}",
        if dirty { "dirty" } else { "clean" }
    );
    println!("cargo:rerun-if-env-changed=HYDRA_BUILD_COMMIT");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
}
