use std::{
    fs,
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use zornmesh_daemon::{DaemonConfig, DaemonErrorCode, DaemonRuntime, DaemonState, ShutdownOutcome};

fn unique_socket(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after epoch")
        .as_nanos();
    let short_name: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(6)
        .collect();
    PathBuf::from("/tmp")
        .join(format!("zm{short_name}-{}-{nanos}", std::process::id()))
        .join("z")
}

fn test_config(path: PathBuf) -> DaemonConfig {
    DaemonConfig::for_test(path)
        .allow_elevated_for_tests(true)
        .with_shutdown_budget(Duration::from_millis(1))
}

#[test]
fn startup_owns_trusted_socket_and_reports_parseable_readiness() {
    let path = unique_socket("ready");
    let daemon = DaemonRuntime::start(test_config(path.clone())).expect("daemon starts");

    assert_eq!(daemon.state(), DaemonState::Ready);
    assert_eq!(
        daemon.readiness_line(),
        format!("zorn: state=ready socket={}", path.display())
    );
    assert!(UnixStream::connect(&path).is_ok(), "daemon owns socket");

    let mode = fs::metadata(&path)
        .expect("socket metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "socket is private to the owning user");
}

#[test]
fn second_start_reports_existing_owner_without_replacing_socket() {
    let path = unique_socket("existing-owner");
    let daemon = DaemonRuntime::start(test_config(path.clone())).expect("first daemon starts");

    let err = DaemonRuntime::start(test_config(path.clone())).expect_err("second start loses");

    assert_eq!(daemon.state(), DaemonState::Ready);
    assert_eq!(err.code(), DaemonErrorCode::ExistingOwner);
    assert!(err.to_string().contains("E_DAEMON_ALREADY_RUNNING"));
    assert!(
        UnixStream::connect(&path).is_ok(),
        "first daemon still owns socket"
    );
}

#[test]
fn stale_socket_left_by_crashed_daemon_is_removed_before_startup() {
    let path = unique_socket("stale");
    let parent = path.parent().expect("socket path has parent");
    fs::create_dir_all(parent).expect("create parent");
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).expect("chmod parent");
    let listener = UnixListener::bind(&path).expect("bind stale socket");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("chmod stale socket");
    drop(listener);
    assert!(
        path.exists(),
        "dropped unix listener leaves stale socket path"
    );

    let daemon =
        DaemonRuntime::start(test_config(path.clone())).expect("daemon removes stale socket");

    assert_eq!(daemon.state(), DaemonState::Ready);
    assert!(UnixStream::connect(&path).is_ok(), "new daemon owns socket");
}

#[test]
fn unsafe_socket_permission_state_is_rejected() {
    let path = unique_socket("unsafe-perms");
    let parent = path.parent().expect("socket path has parent");
    fs::create_dir_all(parent).expect("create parent");
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).expect("chmod parent");
    let listener = UnixListener::bind(&path).expect("bind socket");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).expect("chmod socket");
    drop(listener);

    let err = DaemonRuntime::start(test_config(path.clone())).expect_err("unsafe socket rejected");

    assert_eq!(err.code(), DaemonErrorCode::LocalTrustUnsafe);
    assert!(err.to_string().contains("E_LOCAL_TRUST_UNSAFE"));
    assert!(path.exists(), "unsafe socket is not silently removed");
}

#[test]
fn unsafe_existing_runtime_directory_is_rejected_without_repairing_permissions() {
    let path = unique_socket("unsafe-parent");
    let parent = path.parent().expect("socket path has parent");
    fs::create_dir_all(parent).expect("create parent");
    fs::set_permissions(parent, fs::Permissions::from_mode(0o777)).expect("chmod parent");

    let err = DaemonRuntime::start(test_config(path.clone())).expect_err("unsafe dir rejected");

    let mode = fs::metadata(parent)
        .expect("parent metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(err.code(), DaemonErrorCode::LocalTrustUnsafe);
    assert_eq!(mode, 0o777, "existing shared directories are not chmodded");

    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).expect("repair test dir");
    fs::remove_dir_all(parent).expect("cleanup test dir");
}

#[test]
fn elevated_daemon_start_is_rejected_by_default() {
    let path = unique_socket("root");
    let config = DaemonConfig::for_test(path).with_effective_uid_for_tests(0);

    let err = DaemonRuntime::start(config).expect_err("elevated daemon rejected");

    assert_eq!(err.code(), DaemonErrorCode::ElevatedPrivilege);
    assert!(err.to_string().contains("E_ELEVATED_PRIVILEGE"));
}

#[test]
fn shutdown_reports_draining_and_budget_exceeded_outcome() {
    let path = unique_socket("shutdown");
    let mut daemon = DaemonRuntime::start(test_config(path.clone())).expect("daemon starts");

    let report = daemon
        .shutdown_with_in_flight(1)
        .expect("shutdown report is structured");

    assert_eq!(report.state, DaemonState::Draining);
    assert_eq!(report.outcome, ShutdownOutcome::BudgetExceeded);
    assert!(report.reason.contains("shutdown-budget-exceeded"));
    assert!(!path.exists(), "shutdown releases socket path");
}
