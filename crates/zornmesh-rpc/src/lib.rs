#![doc = "RPC and local transport primitives for zornmesh."]

use std::fmt;

pub const CRATE_BOUNDARY: &str = "zornmesh-rpc";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RpcBoundary;

impl RpcBoundary {
    pub const fn name(self) -> &'static str {
        CRATE_BOUNDARY
    }
}

pub mod local {
    use super::fmt;
    use std::{
        env, fs, io,
        os::unix::{
            fs::{FileTypeExt, MetadataExt, PermissionsExt},
            net::UnixStream,
        },
        path::{Path, PathBuf},
        process::Command,
    };

    pub const ENV_SOCKET_PATH: &str = "ZORN_SOCKET_PATH";
    pub const ENV_NO_AUTOSPAWN: &str = "ZORN_NO_AUTOSPAWN";
    pub const ENV_SHUTDOWN_BUDGET_MS: &str = "ZORN_SHUTDOWN_BUDGET_MS";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LocalErrorCode {
        ExistingOwner,
        LocalTrustUnsafe,
        ElevatedPrivilege,
        DaemonUnreachable,
        InvalidConfig,
        Io,
    }

    impl LocalErrorCode {
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::ExistingOwner => "E_DAEMON_ALREADY_RUNNING",
                Self::LocalTrustUnsafe => "E_LOCAL_TRUST_UNSAFE",
                Self::ElevatedPrivilege => "E_ELEVATED_PRIVILEGE",
                Self::DaemonUnreachable => "E_DAEMON_UNREACHABLE",
                Self::InvalidConfig => "E_INVALID_CONFIG",
                Self::Io => "E_DAEMON_IO",
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct LocalError {
        code: LocalErrorCode,
        message: String,
    }

    impl LocalError {
        pub fn new(code: LocalErrorCode, message: impl Into<String>) -> Self {
            Self {
                code,
                message: message.into(),
            }
        }

        pub const fn code(&self) -> LocalErrorCode {
            self.code
        }

        pub fn message(&self) -> &str {
            &self.message
        }
    }

    impl fmt::Display for LocalError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}: {}", self.code.as_str(), self.message)
        }
    }

    impl std::error::Error for LocalError {}

    pub fn effective_uid() -> Result<u32, LocalError> {
        let status = match fs::read_to_string("/proc/self/status") {
            Ok(status) => status,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return effective_uid_from_id();
            }
            Err(error) => {
                return Err(LocalError::new(
                    LocalErrorCode::Io,
                    format!("failed to inspect process credentials: {error}"),
                ));
            }
        };

        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("Uid:") {
                let mut parts = rest.split_whitespace();
                let _real = parts.next();
                let effective = parts.next().ok_or_else(|| {
                    LocalError::new(
                        LocalErrorCode::Io,
                        "failed to read effective process credentials",
                    )
                })?;
                return effective.parse::<u32>().map_err(|error| {
                    LocalError::new(
                        LocalErrorCode::Io,
                        format!("failed to parse effective process credentials: {error}"),
                    )
                });
            }
        }

        Err(LocalError::new(
            LocalErrorCode::Io,
            "failed to locate effective process credentials",
        ))
    }

    fn effective_uid_from_id() -> Result<u32, LocalError> {
        let output = Command::new("id").arg("-u").output().map_err(|error| {
            LocalError::new(
                LocalErrorCode::Io,
                format!("failed to inspect process credentials with id -u: {error}"),
            )
        })?;
        if !output.status.success() {
            return Err(LocalError::new(
                LocalErrorCode::Io,
                "failed to inspect process credentials with id -u",
            ));
        }
        let raw = String::from_utf8(output.stdout).map_err(|error| {
            LocalError::new(
                LocalErrorCode::Io,
                format!("failed to decode process credentials from id -u: {error}"),
            )
        })?;
        raw.trim().parse::<u32>().map_err(|error| {
            LocalError::new(
                LocalErrorCode::Io,
                format!("failed to parse effective process credentials: {error}"),
            )
        })
    }

    pub fn resolve_socket_path_from_env() -> Result<PathBuf, LocalError> {
        if let Some(path) = env::var_os(ENV_SOCKET_PATH) {
            let path = PathBuf::from(path);
            if path.as_os_str().is_empty() {
                return Err(LocalError::new(
                    LocalErrorCode::InvalidConfig,
                    "ZORN_SOCKET_PATH must not be empty",
                ));
            }
            return Ok(path);
        }

        default_socket_path()
    }

    pub fn default_socket_path() -> Result<PathBuf, LocalError> {
        if let Some(runtime_dir) = env::var_os("XDG_RUNTIME_DIR") {
            return Ok(PathBuf::from(runtime_dir)
                .join("zorn-mesh")
                .join("zorn.sock"));
        }

        #[cfg(target_os = "macos")]
        {
            let base = env::var_os("TMPDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            return Ok(base.join("zorn-mesh.sock"));
        }

        #[cfg(not(target_os = "macos"))]
        Ok(PathBuf::from("/run/user")
            .join(effective_uid()?.to_string())
            .join("zorn-mesh")
            .join("zorn.sock"))
    }

    pub fn ensure_private_parent(path: &Path, uid: u32) -> Result<(), LocalError> {
        let parent = path.parent().ok_or_else(|| {
            LocalError::new(
                LocalErrorCode::InvalidConfig,
                "daemon socket path must include a parent directory",
            )
        })?;

        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|error| {
                LocalError::new(
                    LocalErrorCode::Io,
                    format!("failed to create daemon runtime directory: {error}"),
                )
            })?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|error| {
                LocalError::new(
                    LocalErrorCode::Io,
                    format!("failed to secure daemon runtime directory: {error}"),
                )
            })?;
        }

        let metadata = fs::symlink_metadata(parent).map_err(|error| {
            LocalError::new(
                LocalErrorCode::Io,
                format!("failed to inspect daemon runtime directory: {error}"),
            )
        })?;

        if metadata.uid() != uid {
            return Err(LocalError::new(
                LocalErrorCode::LocalTrustUnsafe,
                "local daemon runtime directory must be owned by the current user",
            ));
        }

        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(LocalError::new(
                LocalErrorCode::LocalTrustUnsafe,
                "local daemon runtime directory must not be accessible by group or other users",
            ));
        }

        Ok(())
    }

    pub fn validate_socket_trust(path: &Path, uid: u32) -> Result<(), LocalError> {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            LocalError::new(
                LocalErrorCode::DaemonUnreachable,
                format!("daemon socket is not reachable: {error}"),
            )
        })?;

        if !metadata.file_type().is_socket() {
            return Err(LocalError::new(
                LocalErrorCode::LocalTrustUnsafe,
                "local daemon endpoint must be a Unix-domain socket owned by the current user",
            ));
        }

        if metadata.uid() != uid {
            return Err(LocalError::new(
                LocalErrorCode::LocalTrustUnsafe,
                "local daemon socket ownership does not match the current user",
            ));
        }

        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(LocalError::new(
                LocalErrorCode::LocalTrustUnsafe,
                "local daemon socket must not be accessible by group or other users",
            ));
        }

        Ok(())
    }

    pub fn connect_trusted_socket(path: &Path, uid: u32) -> Result<UnixStream, LocalError> {
        validate_socket_trust(path, uid)?;
        UnixStream::connect(path).map_err(|error| match error.kind() {
            io::ErrorKind::NotFound
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::TimedOut => LocalError::new(
                LocalErrorCode::DaemonUnreachable,
                "daemon is not accepting connections; run `zornmesh daemon` and retry",
            ),
            _ => LocalError::new(
                LocalErrorCode::DaemonUnreachable,
                format!("failed to connect to daemon: {error}"),
            ),
        })
    }

    pub fn socket_accepts_connections(path: &Path, uid: u32) -> bool {
        connect_trusted_socket(path, uid).is_ok()
    }
}
