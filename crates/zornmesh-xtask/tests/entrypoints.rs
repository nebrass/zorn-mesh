use std::process::Command;

#[test]
fn xtask_exposes_required_entrypoints() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("--help")
        .output()
        .expect("xtask binary runs");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for entrypoint in [
        "check",
        "test",
        "lint",
        "docs",
        "conformance",
        "generate",
        "fixtures",
        "release-preflight",
    ] {
        assert!(
            stdout.contains(entrypoint),
            "xtask help should include {entrypoint}, got:\n{stdout}"
        );
    }
}
