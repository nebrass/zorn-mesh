//! v0.3 contract tests for the parallel-spawn debate engine.
//!
//! Strategy: write fake CLI shell scripts named `claude`, `copilot`,
//! `gemini`, `opencode` into a tempdir and pass the dir as
//! `program_dir_override` (no PATH mutation; the workspace forbids
//! `unsafe`). Audit log goes to a tempdir via `audit_dir_override`.
//! All tests are env-clean and parallel-safe.

use std::{fs, path::PathBuf, time::Duration};

use zornmesh_cli::debate::{
    DEBATE_SCHEMA_VERSION, DebateRunOptions, Platform, Status, run_debate,
};

#[test]
fn run_debate_aggregates_per_platform_results() {
    let env = TestEnv::new("aggregates");
    env.write_fake("claude", r#"#!/usr/bin/env bash
read -r -d '' input || true
printf '%s' "claude critique: looks plausible"
"#);
    env.write_fake("copilot", r#"#!/usr/bin/env bash
read -r -d '' input || true
printf '%s' "copilot critique: missing tests"
"#);
    env.write_fake("gemini", r#"#!/usr/bin/env bash
read -r -d '' input || true
printf '%s' "gemini critique: race condition risk"
"#);
    env.write_fake("opencode", r#"#!/usr/bin/env bash
read -r -d '' input || true
printf '%s' "opencode critique: shipping ready"
"#);

    let outcome = run_debate(env.options("Refactor the payment module"))
        .expect("debate runs");

    assert_eq!(outcome.results.len(), 4);
    assert_eq!(outcome.success_count(), 4);
    let names: Vec<_> = outcome.results.iter().map(|r| r.platform.name()).collect();
    assert!(names.contains(&"claude"));
    assert!(names.contains(&"copilot"));
    assert!(names.contains(&"gemini"));
    assert!(names.contains(&"opencode"));
    for r in &outcome.results {
        assert_eq!(r.status, Status::Success, "{} should succeed", r.platform.name());
        assert!(r.content.contains("critique"));
        assert_eq!(r.exit_code, Some(0));
    }
    assert_eq!(outcome.schema_version, DEBATE_SCHEMA_VERSION);

    let audit_path = outcome.audit_path.as_ref().expect("audit path");
    let audit = fs::read_to_string(audit_path).expect("read audit");
    let lines: Vec<_> = audit.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 6, "audit should have 6 records, got: {audit}");
    assert!(lines[0].contains("\"kind\":\"debate_started\""));
    assert!(lines[5].contains("\"kind\":\"debate_finished\""));
}

#[test]
fn missing_cli_reported_as_status_not_error() {
    let env = TestEnv::new("missing-cli");
    env.write_fake("claude", "#!/usr/bin/env bash\nprintf 'claude says ok'\n");

    let outcome = run_debate(env.options("test plan")).expect("debate runs");

    let claude = outcome
        .results
        .iter()
        .find(|r| r.platform == Platform::Claude)
        .unwrap();
    assert_eq!(claude.status, Status::Success);

    let missing: Vec<_> = outcome
        .results
        .iter()
        .filter(|r| r.status == Status::CliMissing)
        .collect();
    assert_eq!(missing.len(), 3);
    assert_eq!(outcome.missing_count(), 3);
    for r in &missing {
        assert!(
            r.stderr_excerpt.contains("not found on PATH"),
            "stderr_excerpt should mention PATH, got: {}",
            r.stderr_excerpt
        );
    }
}

#[test]
fn slow_cli_is_killed_after_timeout() {
    let env = TestEnv::new("slow-cli");
    env.write_fake("claude", "#!/usr/bin/env bash\nsleep 10\nprintf 'should never appear'\n");
    for name in ["copilot", "gemini", "opencode"] {
        env.write_fake(name, "#!/usr/bin/env bash\nprintf 'ok'\n");
    }

    // Use 5s timeout (vs the slow CLI's 10s sleep) — gives plenty of
    // headroom for the fast scripts under parallel-test load while still
    // reliably killing the slow one.
    let started = std::time::Instant::now();
    let outcome = run_debate(
        env.options("test plan")
            .with_per_platform_timeout(Duration::from_secs(5)),
    )
    .expect("debate completes");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(9),
        "debate should not have run the full 10s sleep: elapsed={elapsed:?}"
    );

    let claude = outcome
        .results
        .iter()
        .find(|r| r.platform == Platform::Claude)
        .unwrap();
    assert_eq!(claude.status, Status::Timeout);

    for name in ["copilot", "gemini", "opencode"] {
        let r = outcome
            .results
            .iter()
            .find(|r| r.platform.name() == name)
            .unwrap();
        assert_eq!(r.status, Status::Success, "{name} should succeed");
    }
}

#[test]
fn nonzero_exit_is_reported_with_stderr() {
    let env = TestEnv::new("nonzero-exit");
    env.write_fake("claude", "#!/usr/bin/env bash\nprintf 'rate limit hit' >&2\nexit 7\n");
    for name in ["copilot", "gemini", "opencode"] {
        env.write_fake(name, "#!/usr/bin/env bash\nprintf 'ok'\n");
    }

    let outcome = run_debate(env.options("test plan")).expect("debate runs");

    let claude = outcome
        .results
        .iter()
        .find(|r| r.platform == Platform::Claude)
        .unwrap();
    assert_eq!(claude.status, Status::NonZeroExit);
    assert_eq!(claude.exit_code, Some(7));
    assert!(claude.stderr_excerpt.contains("rate limit hit"));
}

#[test]
fn empty_response_is_distinct_from_success() {
    let env = TestEnv::new("empty-response");
    env.write_fake("claude", "#!/usr/bin/env bash\n# exit 0 with no output\n");
    for name in ["copilot", "gemini", "opencode"] {
        env.write_fake(name, "#!/usr/bin/env bash\nprintf 'real critique'\n");
    }

    let outcome = run_debate(env.options("test plan")).expect("debate runs");

    let claude = outcome
        .results
        .iter()
        .find(|r| r.platform == Platform::Claude)
        .unwrap();
    assert_eq!(claude.status, Status::EmptyResponse);
    assert_eq!(claude.exit_code, Some(0));
}

#[test]
fn very_large_output_is_truncated() {
    let env = TestEnv::new("truncate");
    env.write_fake(
        "claude",
        "#!/usr/bin/env bash\nprintf 'x%.0s' {1..10000}\n",
    );
    for name in ["copilot", "gemini", "opencode"] {
        env.write_fake(name, "#!/usr/bin/env bash\nprintf 'ok'\n");
    }

    let outcome = run_debate(
        env.options("test plan")
            .with_max_output_bytes(1024),
    )
    .expect("debate runs");

    let claude = outcome
        .results
        .iter()
        .find(|r| r.platform == Platform::Claude)
        .unwrap();
    assert_eq!(claude.status, Status::Success);
    assert!(claude.truncated, "should be marked truncated");
    assert!(claude.bytes_read >= 10_000, "bytes_read should reflect total");
    assert!(claude.content.contains("[truncated"));
}

#[test]
fn empty_plan_is_rejected_before_any_subprocess() {
    let result = run_debate(DebateRunOptions::new("   "));
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E_DEBATE_INVALID_PLAN");
}

#[test]
fn platform_subset_invokes_only_requested() {
    let env = TestEnv::new("subset");
    env.write_fake("claude", "#!/usr/bin/env bash\nprintf 'ok'\n");
    env.write_fake("gemini", "#!/usr/bin/env bash\nprintf 'ok'\n");

    let outcome = run_debate(
        env.options("subset plan")
            .with_platforms(vec![Platform::Claude, Platform::Gemini]),
    )
    .expect("debate runs");

    assert_eq!(outcome.results.len(), 2);
    let names: Vec<_> = outcome.results.iter().map(|r| r.platform.name()).collect();
    assert!(names.contains(&"claude"));
    assert!(names.contains(&"gemini"));
    assert!(!names.contains(&"copilot"));
    assert!(!names.contains(&"opencode"));
}

#[test]
fn audit_records_replay_via_read_audit_in() {
    let env = TestEnv::new("replay");
    for name in ["claude", "copilot", "gemini", "opencode"] {
        env.write_fake(name, "#!/usr/bin/env bash\nprintf 'critique'\n");
    }

    let outcome = run_debate(env.options("replay plan")).expect("debate runs");

    let records =
        zornmesh_cli::debate::read_audit_in(&outcome.debate_id, env.state_dir())
            .expect("read audit");
    assert!(records.len() >= 6);
    assert_eq!(records[0]["kind"], "debate_started");
    assert_eq!(records[0]["debate_id"], outcome.debate_id);
    assert_eq!(
        records.last().expect("last record")["kind"],
        "debate_finished"
    );
}

// ---- TestEnv ----

struct TestEnv {
    bin_dir: PathBuf,
    state_dir: PathBuf,
}

impl TestEnv {
    fn new(label: &str) -> Self {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("zornmesh-debate-test-{label}-{id}"));
        let bin_dir = base.join("bin");
        let state_dir = base.join("state");
        fs::create_dir_all(&bin_dir).expect("mkdir bin");
        fs::create_dir_all(&state_dir).expect("mkdir state");
        Self { bin_dir, state_dir }
    }

    fn write_fake(&self, name: &str, body: &str) {
        let path = self.bin_dir.join(name);
        fs::write(&path, body).expect("write fake CLI");
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
    }

    fn options(&self, plan: &str) -> DebateRunOptions {
        DebateRunOptions::new(plan)
            .with_audit_dir_override(self.state_dir.clone())
            .with_program_dir_override(self.bin_dir.clone())
            .with_per_platform_timeout(Duration::from_secs(10))
    }

    fn state_dir(&self) -> &std::path::Path {
        &self.state_dir
    }
}
