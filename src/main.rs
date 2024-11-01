use actix_otel_example::api::{echo, hello, metrics};
use actix_otel_example::middleware::tracing::record_trace;
use actix_otel_example::telemetry::{build_metrics_provider, init_subscriber};
use actix_otel_example::AppContext;
use actix_web::middleware::{from_fn, Logger};
use actix_web::{web, App, HttpServer};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use opentelemetry::global;
use opentelemetry::global::shutdown_tracer_provider;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    init_subscriber();
    let meter_provider = build_metrics_provider();
    global::set_meter_provider(meter_provider.clone());
    let meter = global::meter("rust-telemetry-example");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppContext::new(meter.clone())))
            // .wrap(Logger::default())
            // .wrap(RequestTracing::new())
            // .wrap(RequestMetrics::default())
            .wrap(from_fn(record_trace))
            .service(hello)
            .service(echo)
            .service(metrics)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await?;

    tokio::task::spawn_blocking(|| shutdown_tracer_provider());
    tokio::task::spawn_blocking(move || meter_provider.shutdown());

    Ok(())
}
