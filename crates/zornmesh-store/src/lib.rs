#![doc = "Durable store crate boundary for future zornmesh storage work."]

pub const CRATE_BOUNDARY: &str = "zornmesh-store";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreBoundary;

impl StoreBoundary {
    pub const fn name(self) -> &'static str {
        CRATE_BOUNDARY
    }
}
