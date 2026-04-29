use std::process::Command;

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(path).expect("fixture exists")
}

#[test]
fn root_help_matches_golden_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_zornmesh"))
        .arg("--help")
        .output()
        .expect("zornmesh binary runs");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        fixture("root-help.stdout")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn trace_help_matches_golden_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_zornmesh"))
        .args(["trace", "--help"])
        .output()
        .expect("zornmesh binary runs");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        fixture("trace-help.stdout")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn bash_completion_matches_golden_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_zornmesh"))
        .args(["completion", "bash"])
        .output()
        .expect("zornmesh binary runs");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        fixture("completion-bash.stdout")
    );
    assert!(output.stderr.is_empty());
}
