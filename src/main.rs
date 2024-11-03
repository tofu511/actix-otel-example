use actix_otel_example::api::{echo, hello, metrics, random};
use actix_otel_example::middleware::metrics::HttpMetrics;
use actix_otel_example::middleware::tracing::record_trace;
use actix_otel_example::telemetry::{build_metrics_provider, init_subscriber};
use actix_otel_example::AppContext;
use actix_web::middleware::{from_fn, Logger};
use actix_web::{web, App, HttpServer};
use opentelemetry::global;
use opentelemetry::global::shutdown_tracer_provider;
use std::sync::Arc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    init_subscriber();
    let meter_provider = build_metrics_provider();
    global::set_meter_provider(meter_provider.clone());
    let meter = Arc::new(global::meter("rust-telemetry-example"));

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppContext::new(meter.clone())))
            .wrap(Logger::default())
            .wrap(from_fn(record_trace))
            .wrap(HttpMetrics::new(meter.clone()))
            .service(hello)
            .service(echo)
            .service(metrics)
            .service(random)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await?;

    tokio::task::spawn_blocking(shutdown_tracer_provider);
    tokio::task::spawn_blocking(move || meter_provider.shutdown());

    Ok(())
}
