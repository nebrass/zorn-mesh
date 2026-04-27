#![doc = "RPC crate boundary for future zornmesh local transport work."]

pub const CRATE_BOUNDARY: &str = "zornmesh-rpc";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RpcBoundary;

impl RpcBoundary {
    pub const fn name(self) -> &'static str {
        CRATE_BOUNDARY
    }
}
