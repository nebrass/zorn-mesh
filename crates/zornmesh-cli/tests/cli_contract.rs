use std::{
    fs,
    io::Write,
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

const TEST_SOCKET: &str = "/tmp/zorn-cli-contract.sock";

fn zornmesh(args: &[&str]) -> Output {
    zornmesh_command(args)
        .output()
        .expect("zornmesh binary runs")
}

fn zornmesh_command(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zornmesh"));
    command
        .args(args)
        .env_remove("NO_COLOR")
        .env_remove("ZORN_SOCKET_PATH")
        .env_remove("XDG_RUNTIME_DIR");
    command
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is utf8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is utf8")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success, got status {:?}, stderr {:?}",
        output.status.code(),
        stderr(output)
    );
    assert!(
        output.stderr.is_empty(),
        "stderr must stay empty on success"
    );
}

fn assert_no_ansi(text: &str) {
    assert!(
        !text.contains("\u{1b}["),
        "output must not contain ANSI escapes"
    );
}

fn read_json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("stdout is valid JSON")
}

fn assert_read_json_contract(value: &serde_json::Value, command: &str) {
    let object = value.as_object().expect("read response is a JSON object");
    assert_eq!(object.len(), 5);
    for key in ["schema_version", "command", "status", "data", "warnings"] {
        assert!(object.contains_key(key), "missing top-level key {key}");
    }
    assert_eq!(value["schema_version"], "zornmesh.cli.read.v1");
    assert_eq!(value["command"], command);
    assert_eq!(value["status"], "ok");
    assert!(value["data"].is_object(), "data must be an object");
    assert!(value["warnings"].is_array(), "warnings must be an array");
}

fn unique_socket(name: &str) -> PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let short_name: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(6)
        .collect();
    PathBuf::from("/tmp")
        .join(format!("zm{short_name}-{}-{id}", std::process::id()))
        .join("z")
}

fn healthy_socket(name: &str) -> (UnixListener, PathBuf) {
    let path = unique_socket(name);
    let parent = path.parent().expect("socket path has parent");
    fs::create_dir_all(parent).expect("socket parent created");
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).expect("socket parent secured");
    let listener = UnixListener::bind(&path).expect("socket listener binds");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("socket secured");
    (listener, path)
}

fn temp_config(contents: &str) -> std::path::PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("zornmesh-cli-contract-{}-{id}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp config dir created");
    let path = dir.join("zornmesh.conf");
    fs::write(&path, contents).expect("temp config written");
    path
}

#[test]
fn read_json_outputs_are_parseable_and_share_top_level_shape() {
    let cases = [
        (vec!["--help", "--output", "json"], "help"),
        (vec!["--version", "--output", "json"], "version"),
        (
            vec![
                "daemon",
                "status",
                "--socket",
                TEST_SOCKET,
                "--output",
                "json",
            ],
            "daemon status",
        ),
        (vec!["daemon", "--help", "--output", "json"], "daemon help"),
        (vec!["agents", "--output", "json"], "agents"),
        (vec!["agents", "--help", "--output", "json"], "agents help"),
        (vec!["doctor", "--output", "json"], "doctor"),
        (vec!["doctor", "--help", "--output", "json"], "doctor help"),
        (vec!["trace", "--help", "--output", "json"], "trace help"),
    ];

    for (args, command) in cases {
        let output = zornmesh(&args);

        assert_success(&output);
        let text = stdout(&output);
        assert_no_ansi(&text);
        assert!(!text.contains("zornmesh doctor\n"));
        assert_read_json_contract(&read_json(&output), command);
    }
}

#[test]
fn streaming_json_outputs_are_parseable_ndjson_events() {
    for format in ["json", "ndjson"] {
        let output = zornmesh(&["trace", "events", "--output", format]);

        assert_success(&output);
        for (index, line) in stdout(&output).lines().enumerate() {
            let event: serde_json::Value =
                serde_json::from_str(line).expect("NDJSON line is valid JSON");
            let object = event.as_object().expect("event is a JSON object");
            assert_eq!(object.len(), 4);
            for key in ["schema_version", "event_type", "sequence", "data"] {
                assert!(object.contains_key(key), "missing event key {key}");
            }
            assert_eq!(event["schema_version"], "zornmesh.cli.event.v1");
            assert_eq!(event["event_type"], "trace.scaffolded");
            assert_eq!(event["sequence"], (index + 1) as u64);
            assert!(event["data"].is_object(), "event data must be an object");
        }
    }
}

#[test]
fn read_commands_emit_stable_human_stdout() {
    let cases = [
        (
            vec!["daemon", "status", "--socket", TEST_SOCKET],
            format!(
                "zornmesh daemon status\nstate: unreachable\nsocket: {TEST_SOCKET}\nsocket_source: cli\nremediation: start the daemon with `zornmesh daemon --socket {TEST_SOCKET}`\n"
            ),
        ),
        (
            vec!["agents", "--socket", TEST_SOCKET],
            "zornmesh agents\nstatus: unavailable\nagents: 0\nwarning: agent registry is not available in this scaffold\nremediation: connect agents after identity registration is enabled\n"
                .to_string(),
        ),
        (
            vec!["doctor", "--socket", TEST_SOCKET],
            format!(
                "zornmesh doctor\nstatus: degraded\ndaemon: unreachable\nversion: 0.1.0\nsocket: {TEST_SOCKET}\nsocket_source: cli\nsocket_ownership: unavailable\nsocket_permissions: unavailable\nschema: available (zornmesh.cli.doctor.v1)\notel: unavailable\nsignature: unverifiable\nsbom: unavailable\ntrust: degraded\nshutdown: unavailable\nremediation: start the daemon with `zornmesh daemon --socket {TEST_SOCKET}`\n"
            ),
        ),
    ];

    for (args, expected) in cases {
        let output = zornmesh(&args);

        assert_success(&output);
        assert_eq!(stdout(&output), expected);
        assert_no_ansi(&stdout(&output));
    }
}

#[test]
fn json_read_commands_emit_only_stable_json_stdout() {
    let cases = [
        (
            vec!["daemon", "status", "--socket", TEST_SOCKET, "--output", "json"],
            format!(
                "{{\"schema_version\":\"zornmesh.cli.read.v1\",\"command\":\"daemon status\",\"status\":\"ok\",\"data\":{{\"daemon_state\":\"unreachable\",\"socket_path\":\"{TEST_SOCKET}\",\"socket_source\":\"cli\"}},\"warnings\":[]}}\n"
            ),
        ),
        (
            vec!["agents", "--socket", TEST_SOCKET, "--output", "json"],
            "{\"schema_version\":\"zornmesh.cli.read.v1\",\"command\":\"agents\",\"status\":\"ok\",\"data\":{\"registry_status\":\"unavailable\",\"agents\":[]},\"warnings\":[{\"code\":\"W_AGENT_REGISTRY_UNAVAILABLE\",\"message\":\"agent registry is not available in this scaffold\"}]}\n"
                .to_string(),
        ),
        (
            vec!["doctor", "--socket", TEST_SOCKET, "--output", "json"],
            format!(
                "{{\"schema_version\":\"zornmesh.cli.read.v1\",\"command\":\"doctor\",\"status\":\"ok\",\"data\":{{\"health\":\"degraded\",\"diagnostics_schema\":\"zornmesh.cli.doctor.v1\",\"daemon\":{{\"status\":\"unreachable\",\"version\":\"0.1.0\",\"socket_path\":\"{TEST_SOCKET}\",\"socket_source\":\"cli\",\"remediation\":\"start the daemon with `zornmesh daemon --socket {TEST_SOCKET}`\"}},\"socket\":{{\"ownership\":\"unavailable\",\"permissions\":\"unavailable\"}},\"schema\":{{\"status\":\"available\",\"version\":\"zornmesh.cli.doctor.v1\"}},\"otel\":{{\"status\":\"unavailable\",\"endpoint\":\"unconfigured\"}},\"signature\":{{\"status\":\"unverifiable\",\"identity\":\"unavailable\"}},\"sbom\":{{\"status\":\"unavailable\",\"identity\":\"unavailable\"}},\"trust\":{{\"status\":\"degraded\",\"posture\":\"daemon-unreachable\"}},\"shutdown\":{{\"status\":\"unavailable\",\"in_flight_work\":\"unavailable\"}}}},\"warnings\":[{{\"code\":\"W_DAEMON_UNREACHABLE\",\"message\":\"daemon is unreachable; start the daemon or choose another socket\"}},{{\"code\":\"W_OTEL_UNAVAILABLE\",\"message\":\"OTel reachability evidence is not configured for this build\"}},{{\"code\":\"W_SIGNATURE_UNVERIFIABLE\",\"message\":\"build signature evidence is unavailable for this build\"}},{{\"code\":\"W_SBOM_UNAVAILABLE\",\"message\":\"SBOM identity evidence is unavailable for this build\"}}]}}\n"
            ),
        ),
    ];

    for (args, expected) in cases {
        let output = zornmesh(&args);

        assert_success(&output);
        let text = stdout(&output);
        assert_eq!(text, expected);
        assert!(text.starts_with('{'));
        assert!(text.ends_with("}\n"));
        assert_no_ansi(&text);
        assert!(!text.contains("zornmesh doctor\n"));
        assert!(!text.contains("zornmesh agents\n"));
        assert!(!text.contains("zornmesh daemon status\n"));
    }
}

#[test]
fn streaming_json_mode_emits_ndjson_events() {
    let output = zornmesh(&["trace", "events", "--output", "json"]);

    assert_success(&output);
    assert_eq!(
        stdout(&output),
        "{\"schema_version\":\"zornmesh.cli.event.v1\",\"event_type\":\"trace.scaffolded\",\"sequence\":1,\"data\":{\"status\":\"no_events\"}}\n"
    );
    for line in stdout(&output).lines() {
        assert!(line.starts_with('{'));
        assert!(line.ends_with('}'));
        assert!(line.contains("\"schema_version\":\"zornmesh.cli.event.v1\""));
        assert!(line.contains("\"event_type\":"));
        assert!(line.contains("\"sequence\":"));
        assert!(line.contains("\"data\":"));
    }
}

#[test]
fn doctor_healthy_json_reports_required_diagnostic_categories() {
    let (_listener, path) = healthy_socket("doctor-healthy");
    let socket = path.to_str().expect("socket path is utf8");
    let output = zornmesh(&["doctor", "--socket", socket, "--output", "json"]);

    assert_success(&output);
    let value = read_json(&output);
    assert_read_json_contract(&value, "doctor");
    let data = &value["data"];
    assert_eq!(data["health"], "degraded");
    assert_eq!(data["diagnostics_schema"], "zornmesh.cli.doctor.v1");
    assert_eq!(data["daemon"]["status"], "ready");
    assert_eq!(data["daemon"]["version"], "0.1.0");
    assert_eq!(data["daemon"]["socket_path"], socket);
    assert_eq!(data["socket"]["ownership"], "current-user");
    assert_eq!(data["socket"]["permissions"], "private");
    assert_eq!(data["schema"]["status"], "available");
    assert_eq!(data["otel"]["status"], "unavailable");
    assert_eq!(data["signature"]["status"], "unverifiable");
    assert_eq!(data["sbom"]["status"], "unavailable");
    assert_eq!(data["trust"]["status"], "trusted");
    assert_eq!(data["shutdown"]["status"], "idle");
    assert_eq!(
        value["warnings"].as_array().expect("warnings array").len(),
        3
    );
}

#[test]
fn doctor_unsafe_socket_reports_blocked_trust_status() {
    let (_listener, path) = healthy_socket("doctor-unsafe");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).expect("socket made unsafe");
    let socket = path.to_str().expect("socket path is utf8");
    let output = zornmesh(&["doctor", "--socket", socket, "--output", "json"]);

    assert_success(&output);
    let value = read_json(&output);
    let data = &value["data"];
    assert_eq!(data["daemon"]["status"], "blocked");
    assert_eq!(data["socket"]["ownership"], "current-user");
    assert_eq!(data["socket"]["permissions"], "unsafe");
    assert_eq!(data["trust"]["status"], "unsafe");
    assert_eq!(data["trust"]["posture"], "unsafe-socket");
    assert_eq!(value["warnings"][0]["code"], "W_LOCAL_TRUST_UNSAFE");
}

#[test]
fn daemon_shutdown_non_interactive_reports_unreachable_status() {
    let output = zornmesh(&[
        "daemon",
        "shutdown",
        "--socket",
        TEST_SOCKET,
        "--non-interactive",
    ]);

    assert_success(&output);
    assert_eq!(
        stdout(&output),
        format!(
            "zornmesh daemon shutdown\nstate: unreachable\noutcome: not-running\nsocket: {TEST_SOCKET}\nshutdown_budget_ms: 10000\nin_flight_work: unavailable\nremediation: start the daemon with `zornmesh daemon --socket {TEST_SOCKET}`\n"
        )
    );
    assert_no_ansi(&stdout(&output));
}

#[test]
fn daemon_shutdown_json_reports_stable_outcome() {
    let output = zornmesh(&[
        "daemon",
        "shutdown",
        "--socket",
        TEST_SOCKET,
        "--non-interactive",
        "--output",
        "json",
    ]);

    assert_success(&output);
    assert_eq!(
        stdout(&output),
        format!(
            "{{\"schema_version\":\"zornmesh.cli.read.v1\",\"command\":\"daemon shutdown\",\"status\":\"ok\",\"data\":{{\"daemon_state\":\"unreachable\",\"outcome\":\"not-running\",\"socket_path\":\"{TEST_SOCKET}\",\"shutdown_budget_ms\":10000,\"in_flight_work\":\"unavailable\",\"remediation\":\"start the daemon with `zornmesh daemon --socket {TEST_SOCKET}`\"}},\"warnings\":[]}}\n"
        )
    );
}

#[test]
fn supported_shell_completion_contains_initial_commands_and_flags() {
    for shell in ["bash", "zsh", "fish"] {
        let output = zornmesh(&["completion", shell]);

        assert_success(&output);
        let text = stdout(&output);
        assert_no_ansi(&text);
        assert!(text.contains("zornmesh"), "completion names binary");
        for token in [
            "daemon",
            "doctor",
            "agents",
            "stdio",
            "help",
            "--output",
            "--socket",
            "--non-interactive",
        ] {
            assert!(
                text.contains(token),
                "{shell} completion must include {token}"
            );
        }
    }
}

#[test]
fn stdio_daemon_unavailable_reports_protocol_error_to_host() {
    let output = zornmesh(&[
        "stdio",
        "--as-agent",
        "agent.local/mcp-host",
        "--socket",
        TEST_SOCKET,
    ]);

    assert!(
        output.status.success(),
        "protocol errors are reported on stdout for MCP hosts, got stderr {:?}",
        stderr(&output)
    );
    assert!(output.stderr.is_empty());
    let value = read_json(&output);
    assert_eq!(value["jsonrpc"], "2.0");
    assert!(value["id"].is_null());
    assert_eq!(value["error"]["data"]["code"], "E_DAEMON_UNREACHABLE");
    assert!(!stdout(&output).contains(TEST_SOCKET));
}

#[test]
fn stdio_initialize_over_ready_socket_emits_mcp_ack() {
    let (_listener, path) = healthy_socket("stdio-init");
    let socket = path.to_str().expect("socket path is utf8");
    let mut child = zornmesh_command(&[
        "stdio",
        "--as-agent",
        "agent.local/mcp-host",
        "--socket",
        socket,
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("zornmesh stdio starts");
    {
        let stdin = child.stdin.as_mut().expect("stdin is piped");
        stdin
            .write_all(
                br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
            )
            .expect("initialize written");
        stdin.write_all(b"\n").expect("newline written");
    }

    let output = child.wait_with_output().expect("stdio exits after EOF");
    assert_success(&output);
    let value = read_json(&output);
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 1);
    assert_eq!(value["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(value["result"]["serverInfo"]["name"], "zornmesh-stdio");
}

#[test]
fn unsupported_shell_completion_fails_without_stdout() {
    let output = zornmesh(&["completion", "powershell"]);

    assert_eq!(output.status.code(), Some(64));
    assert!(output.stdout.is_empty());
    assert_eq!(
        stderr(&output),
        "E_UNSUPPORTED_SHELL: unsupported shell 'powershell'; supported shells: bash, zsh, fish\n"
    );
}

#[test]
fn no_color_keeps_human_plain_and_json_byte_identical() {
    let mut human = zornmesh_command(&["doctor", "--socket", TEST_SOCKET]);
    human.env("NO_COLOR", "1");
    let human = human.output().expect("zornmesh binary runs");
    assert_success(&human);
    assert_no_ansi(&stdout(&human));

    let json = zornmesh(&["doctor", "--socket", TEST_SOCKET, "--output", "json"]);
    let mut json_no_color =
        zornmesh_command(&["doctor", "--socket", TEST_SOCKET, "--output", "json"]);
    json_no_color.env("NO_COLOR", "1");
    let json_no_color = json_no_color.output().expect("zornmesh binary runs");

    assert_success(&json);
    assert_success(&json_no_color);
    assert_eq!(json.stdout, json_no_color.stdout);
}

#[test]
fn exit_contract_maps_registered_error_categories() {
    let cases = [
        (
            vec!["missing"],
            64,
            "E_UNSUPPORTED_COMMAND: unsupported zornmesh command 'missing'\n",
        ),
        (
            vec![
                "daemon",
                "status",
                "--socket",
                TEST_SOCKET,
                "--require-ready",
            ],
            69,
            "E_DAEMON_UNREACHABLE: daemon is unreachable at /tmp/zorn-cli-contract.sock; start the daemon or choose another socket\n",
        ),
        (
            vec!["agents", "inspect", ""],
            65,
            "E_VALIDATION_FAILED: agent id must not be empty\n",
        ),
        (
            vec!["agents", "inspect", "missing-agent"],
            66,
            "E_NOT_FOUND: agent 'missing-agent' was not found\n",
        ),
        (
            vec!["doctor", "--output", "yaml"],
            64,
            "E_UNSUPPORTED_OUTPUT_FORMAT: unsupported output format 'yaml'; supported formats: human, json, ndjson\n",
        ),
    ];

    for (args, code, expected_stderr) in cases {
        let output = zornmesh(&args);

        assert_eq!(output.status.code(), Some(code), "args: {args:?}");
        assert!(
            output.stdout.is_empty(),
            "stdout must stay empty on failure"
        );
        assert_eq!(stderr(&output), expected_stderr);
    }
}

#[test]
fn configuration_precedence_is_deterministic() {
    let config = temp_config("socket_path=/tmp/zorn-config.sock\n");
    let config = config.to_str().expect("temp path is utf8");

    let config_output = zornmesh(&["daemon", "status", "--config", config, "--output", "json"]);
    assert_success(&config_output);
    assert!(stdout(&config_output).contains("\"socket_path\":\"/tmp/zorn-config.sock\""));
    assert!(stdout(&config_output).contains("\"socket_source\":\"config\""));

    let mut env_command =
        zornmesh_command(&["daemon", "status", "--config", config, "--output", "json"]);
    env_command.env("ZORN_SOCKET_PATH", "/tmp/zorn-env.sock");
    let env_output = env_command.output().expect("zornmesh binary runs");
    assert_success(&env_output);
    assert!(stdout(&env_output).contains("\"socket_path\":\"/tmp/zorn-env.sock\""));
    assert!(stdout(&env_output).contains("\"socket_source\":\"env\""));

    let mut cli_command = zornmesh_command(&[
        "daemon",
        "status",
        "--config",
        config,
        "--socket",
        "/tmp/zorn-cli.sock",
        "--output",
        "json",
    ]);
    cli_command.env("ZORN_SOCKET_PATH", "/tmp/zorn-env.sock");
    let cli_output = cli_command.output().expect("zornmesh binary runs");
    assert_success(&cli_output);
    assert!(stdout(&cli_output).contains("\"socket_path\":\"/tmp/zorn-cli.sock\""));
    assert!(stdout(&cli_output).contains("\"socket_source\":\"cli\""));
}
