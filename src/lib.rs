use opentelemetry::metrics::Meter;
use std::sync::Arc;

pub mod api;
pub mod middleware;
pub mod telemetry;

#[derive(Debug)]
pub struct AppContext {
    meter: Arc<Meter>,
}

impl AppContext {
    pub fn new(meter: Arc<Meter>) -> Self {
        Self { meter }
    }
}
