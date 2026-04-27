use std::process::Command;

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/cli")
        .join(name);
    std::fs::read_to_string(path).expect("fixture exists")
}

#[test]
fn daemon_help_matches_golden_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_zornmesh"))
        .args(["daemon", "--help"])
        .output()
        .expect("zornmesh binary runs");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        fixture("daemon-help.stdout")
    );
    assert!(output.stderr.is_empty());
}
