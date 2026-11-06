//! Builds the stub plugin (`stub/main.rs`, std only) with the compiler cargo uses, so
//! the integration tests run a real program on every system; its path is
//! `LOCI_STUB_EXE` (research 24).

fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let exe = out.join(format!("loci-stub{}", std::env::consts::EXE_SUFFIX));
    let status = std::process::Command::new(std::env::var_os("RUSTC").unwrap())
        .args(["--edition", "2024", "stub/main.rs", "-o"])
        .arg(&exe)
        .status()
        .unwrap();
    assert!(status.success(), "building the stub plugin failed");
    println!("cargo::rerun-if-changed=stub/main.rs");
    println!("cargo::rustc-env=LOCI_STUB_EXE={}", exe.display());
}
