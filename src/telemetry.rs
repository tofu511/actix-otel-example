use crate::OtelConfig;
use once_cell::sync::Lazy;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{ExportConfig, WithExportConfig};
use opentelemetry_sdk::logs::LoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::{RandomIdGenerator, Tracer, TracerProvider};
use opentelemetry_sdk::Resource;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

static RESOURCE: Lazy<Resource> = Lazy::new(|| {
    Resource::new(vec![KeyValue::new(
        opentelemetry_semantic_conventions::resource::SERVICE_NAME,
        "rust-telemetry-example",
    )])
});

fn init_stdout_tracer() -> Tracer {
    TracerProvider::builder()
        .with_simple_exporter(opentelemetry_stdout::SpanExporter::default())
        .with_config(opentelemetry_sdk::trace::Config::default().with_resource(RESOURCE.clone()))
        .build()
        .tracer("stdout")
}

fn init_tracer(otel_config: &OtelConfig) -> Tracer {
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_trace_config(
            opentelemetry_sdk::trace::Config::default()
                .with_resource(RESOURCE.clone())
                .with_id_generator(RandomIdGenerator::default()),
        )
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(otel_config.endpoint.clone())
                .with_timeout(std::time::Duration::from_secs(5)),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .inspect_err(|e| println!("{:#?}", e))
        .unwrap()
        .tracer("sample_tracer")
}

pub fn build_metrics_provider(otel_config: &OtelConfig) -> SdkMeterProvider {
    let export_config = ExportConfig {
        endpoint: otel_config.endpoint.clone(),
        ..ExportConfig::default()
    };
    opentelemetry_otlp::new_pipeline()
        .metrics(opentelemetry_sdk::runtime::Tokio)
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_timeout(std::time::Duration::from_secs(2))
                .with_export_config(export_config),
        )
        .with_resource(RESOURCE.clone())
        .build()
        .expect("failed to init metrics")
}

fn init_logs(otel_config: &OtelConfig) -> LoggerProvider {
    opentelemetry_otlp::new_pipeline()
        .logging()
        .with_resource(RESOURCE.clone())
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(otel_config.endpoint.clone())
                .with_timeout(std::time::Duration::from_secs(2)),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .expect("failed to init logger provider")
}

pub fn init_subscriber(otel_config: &OtelConfig) {
    // let std_tracer = init_stdout_tracer();
    // let stdout_layer = tracing_opentelemetry::layer().with_tracer(std_tracer);
    let tracer = init_tracer(otel_config);
    let trace_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let logger = init_logs(otel_config);
    let logger_layer = OpenTelemetryTracingBridge::new(&logger);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::Layer::new()
                .with_target(true)
                .with_span_events(FmtSpan::ACTIVE)
                .compact(),
        )
        .with(tracing_subscriber::filter::LevelFilter::INFO)
        // .with(stdout_layer)
        .with(trace_layer)
        .with(logger_layer)
        .init();
}
