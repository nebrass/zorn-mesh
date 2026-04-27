#![doc = "Daemon crate boundary for future zornmesh runtime work."]

pub const CRATE_BOUNDARY: &str = "zornmesh-daemon";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaemonBoundary;

impl DaemonBoundary {
    pub const fn name(self) -> &'static str {
        CRATE_BOUNDARY
    }
}
