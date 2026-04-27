#![doc = "Rust SDK crate boundary for zornmesh agents."]

pub const CRATE_BOUNDARY: &str = "zornmesh-sdk";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdkBoundary;

impl SdkBoundary {
    pub const fn name(self) -> &'static str {
        CRATE_BOUNDARY
    }
}
