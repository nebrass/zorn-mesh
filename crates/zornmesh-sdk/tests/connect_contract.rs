use std::{
    fs,
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::PathBuf,
    sync::{Arc, Barrier},
    thread,
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};

use zornmesh_daemon::{DaemonConfig, DaemonRuntime};
use zornmesh_sdk::{ConnectOptions, Mesh, SdkErrorCode};

fn unique_socket(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after epoch")
        .as_nanos();
    std::env::temp_dir()
        .join(format!(
            "zornmesh-sdk-{name}-{}-{nanos}",
            std::process::id()
        ))
        .join("zorn.sock")
}

struct AutoSpawnCleanup {
    path: PathBuf,
}

impl AutoSpawnCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for AutoSpawnCleanup {
    fn drop(&mut self) {
        Mesh::shutdown_autospawned_daemon_for_tests(&self.path);
        if let Some(parent) = self.path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }
}

fn autospawn_options(path: PathBuf) -> ConnectOptions {
    ConnectOptions::for_socket(path)
        .allow_elevated_daemon_for_tests()
        .with_connect_timeout(Duration::from_millis(200))
}

#[test]
fn no_autospawn_env_returns_daemon_unreachable_without_creating_socket() {
    let path = unique_socket("no-autospawn");
    let options = ConnectOptions::from_env_pairs([
        ("ZORN_SOCKET_PATH", path.to_string_lossy().to_string()),
        ("ZORN_NO_AUTOSPAWN", "1".to_owned()),
    ])
    .expect("env config parses");

    let err = Mesh::connect_with_options(options).expect_err("connect fails without spawning");

    assert_eq!(err.code(), SdkErrorCode::DaemonUnreachable);
    assert!(err.to_string().contains("E_DAEMON_UNREACHABLE"));
    assert!(err.to_string().contains("zornmesh daemon"));
    assert!(
        !path.exists(),
        "disabled auto-spawn does not create daemon socket"
    );
}

#[test]
fn auto_spawn_enabled_starts_daemon_and_connects_within_budget() {
    let path = unique_socket("autospawn");
    let _cleanup = AutoSpawnCleanup::new(path.clone());
    let options = autospawn_options(path.clone());

    let mesh = Mesh::connect_with_options(options).expect("sdk auto-spawns daemon");

    assert_eq!(mesh.socket_path(), path.as_path());
    assert!(
        path.exists(),
        "auto-spawned daemon owns the resolved socket"
    );
    assert!(
        Mesh::has_autospawned_daemon_for_tests(&path),
        "sdk records the daemon it auto-spawned for cleanup and race control"
    );
}

#[test]
fn existing_ready_daemon_is_reused_without_spawning_another_daemon() {
    let path = unique_socket("existing-ready");
    let parent = path.parent().expect("socket path has parent").to_path_buf();
    let _daemon =
        DaemonRuntime::start(DaemonConfig::for_test(path.clone()).allow_elevated_for_tests(true))
            .expect("daemon starts");
    let options = autospawn_options(path.clone());

    let mesh = Mesh::connect_with_options(options).expect("sdk connects to existing daemon");

    assert_eq!(mesh.socket_path(), path.as_path());
    assert!(
        !Mesh::has_autospawned_daemon_for_tests(&path),
        "existing daemon connection does not create an SDK-owned daemon"
    );
    drop(_daemon);
    fs::remove_dir_all(parent).expect("cleanup daemon dir");
}

#[test]
fn readiness_timeout_returns_retryable_typed_error_and_cleans_up() {
    let path = unique_socket("timeout");
    let _cleanup = AutoSpawnCleanup::new(path.clone());
    let options = autospawn_options(path.clone())
        .with_connect_timeout(Duration::from_millis(1))
        .with_daemon_start_delay_for_tests(Duration::from_millis(50));

    let err = Mesh::connect_with_options(options).expect_err("connect times out");

    assert_eq!(err.code(), SdkErrorCode::DaemonReadinessTimeout);
    assert!(err.retryable());
    assert!(err.to_string().contains("E_DAEMON_READINESS_TIMEOUT"));
    assert!(
        !Mesh::has_autospawned_daemon_for_tests(&path),
        "timed-out startup does not leave an SDK-owned daemon behind"
    );
}

#[test]
fn concurrent_connect_attempts_share_one_autospawned_daemon() {
    let path = unique_socket("concurrent");
    let _cleanup = AutoSpawnCleanup::new(path.clone());
    let threads = 10;
    let barrier = Arc::new(Barrier::new(threads));
    let handles = (0..threads)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let options = autospawn_options(path.clone());
            thread::spawn(move || {
                barrier.wait();
                Mesh::connect_with_options(options)
            })
        })
        .collect::<Vec<_>>();

    let meshes = handles
        .into_iter()
        .map(|handle| handle.join().expect("connect thread joins"))
        .collect::<Result<Vec<_>, _>>()
        .expect("all concurrent connects succeed");

    assert_eq!(meshes.len(), threads);
    assert!(
        meshes
            .iter()
            .all(|mesh| mesh.socket_path() == path.as_path())
    );
    assert_eq!(
        Mesh::autospawned_daemon_count_for_tests(&path),
        1,
        "concurrent SDK connects converge on one daemon instance"
    );
}

#[test]
fn shared_connect_contract_fixture_pins_state_error_and_timeout_names() {
    let fixture = include_str!("../../../fixtures/sdk/connect-contract.json");

    assert!(fixture.contains("\"ready\""));
    assert!(fixture.contains("\"draining\""));
    assert!(fixture.contains("\"E_DAEMON_UNREACHABLE\""));
    assert!(fixture.contains("\"E_DAEMON_READINESS_TIMEOUT\""));
    assert!(fixture.contains("\"connect_timeout_ms\": 1000"));
}

#[test]
fn unsafe_socket_permission_state_is_rejected_during_client_validation() {
    let path = unique_socket("unsafe-client");
    let parent = path.parent().expect("socket path has parent");
    fs::create_dir_all(parent).expect("create parent");
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).expect("chmod parent");
    let listener = UnixListener::bind(&path).expect("bind socket");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).expect("chmod socket");
    drop(listener);

    let options = ConnectOptions::for_socket(path).without_auto_spawn();
    let err = Mesh::connect_with_options(options).expect_err("unsafe socket rejected");

    assert_eq!(err.code(), SdkErrorCode::LocalTrustUnsafe);
    assert!(err.to_string().contains("E_LOCAL_TRUST_UNSAFE"));
}
