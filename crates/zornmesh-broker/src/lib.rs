#![doc = "Broker crate boundary for future zornmesh routing work."]

pub const CRATE_BOUNDARY: &str = "zornmesh-broker";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokerBoundary;

impl BrokerBoundary {
    pub const fn name(self) -> &'static str {
        CRATE_BOUNDARY
    }
}
