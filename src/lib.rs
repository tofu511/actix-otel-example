use opentelemetry::metrics::Meter;

pub mod api;
pub mod middleware;
pub mod telemetry;

#[derive(Debug)]
pub struct AppContext {
    meter: Meter,
}

impl AppContext {
    pub fn new(meter: Meter) -> Self {
        Self { meter }
    }
}
