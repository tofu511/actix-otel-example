use opentelemetry::metrics::Meter;
use serde::Deserialize;
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

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub otel_config: OtelConfig,
}

#[derive(Debug, Deserialize)]
pub struct OtelConfig {
    pub endpoint: String,
}
