use actix_web::body::{BodySize, MessageBody};
use actix_web::dev::{self, ServiceRequest, ServiceResponse};
use actix_web::http::header::CONTENT_LENGTH;
use futures_util::future;
use futures_util::future::LocalBoxFuture;
use opentelemetry::metrics::{Histogram, Meter, UpDownCounter};
use opentelemetry::KeyValue;
use opentelemetry_semantic_conventions::trace::{
    HTTP_REQUEST_METHOD, HTTP_RESPONSE_STATUS_CODE, HTTP_ROUTE, URL_SCHEME,
};
use std::sync::Arc;
use std::time::SystemTime;

const HTTP_SERVER_DURATION: &str = "http.server.duration";
const HTTP_SERVER_ACTIVE_REQUESTS: &str = "http.server.active_requests";
const HTTP_SERVER_REQUEST_SIZE: &str = "http.server.request.size";
const HTTP_SERVER_RESPONSE_SIZE: &str = "http.server.response.size";

#[derive(Clone, Debug)]
pub struct Metrics {
    http_server_duration: Histogram<f64>,
    http_server_active_requests: UpDownCounter<i64>,
    http_server_request_size: Histogram<u64>,
    http_server_response_size: Histogram<u64>,
}

impl Metrics {
    fn new(meter: Arc<Meter>) -> Self {
        let http_server_duration = meter
            .f64_histogram(HTTP_SERVER_DURATION)
            .with_description("Measures the duration of inbound HTTP requests.")
            .with_unit("s")
            .init();

        let http_server_active_requests = meter
            .i64_up_down_counter(HTTP_SERVER_ACTIVE_REQUESTS)
            .with_description(
                "Measures the number of concurrent HTTP requests that are currently in-flight.",
            )
            .init();

        let http_server_request_size = meter
            .u64_histogram(HTTP_SERVER_REQUEST_SIZE)
            .with_description("Measures the size of HTTP request messages (compressed).")
            .with_unit("By")
            .init();

        let http_server_response_size = meter
            .u64_histogram(HTTP_SERVER_RESPONSE_SIZE)
            .with_description("Measures the size of HTTP response messages (compressed).")
            .with_unit("By")
            .init();

        Metrics {
            http_server_active_requests,
            http_server_duration,
            http_server_request_size,
            http_server_response_size,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HttpMetrics {
    meter: Arc<Meter>,
}

impl HttpMetrics {
    pub fn new(meter: Arc<Meter>) -> Self {
        Self { meter }
    }
}

impl<S, B> dev::Transform<S, dev::ServiceRequest> for HttpMetrics
where
    S: dev::Service<
        dev::ServiceRequest,
        Response = dev::ServiceResponse<B>,
        Error = actix_web::Error,
    >,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = dev::ServiceResponse<B>;
    type Error = actix_web::Error;
    type Transform = HttpMetricsMiddleware<S>;
    type InitError = ();
    type Future = future::Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        let service = HttpMetricsMiddleware {
            service,
            meter: self.meter.clone(),
        };

        future::ok(service)
    }
}

pub struct HttpMetricsMiddleware<S> {
    service: S,
    meter: Arc<Meter>,
}
impl<S, B> dev::Service<dev::ServiceRequest> for HttpMetricsMiddleware<S>
where
    S: dev::Service<
        dev::ServiceRequest,
        Response = dev::ServiceResponse<B>,
        Error = actix_web::Error,
    >,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = dev::ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    dev::forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let metrics = Metrics::new(self.meter.clone());
        let timer = SystemTime::now();
        let mut attributes = Vec::new();
        let request_method = req.method();

        attributes.push(KeyValue::new(
            HTTP_REQUEST_METHOD,
            request_method.to_string(),
        ));
        attributes.push(KeyValue::new(
            URL_SCHEME,
            req.connection_info().scheme().to_string(),
        ));

        metrics
            .http_server_active_requests
            .add(1, attributes.as_slice());
        attributes.push(KeyValue::new(
            HTTP_ROUTE,
            req.match_pattern().unwrap_or_default(),
        ));

        let request_size = req
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|len| len.to_str().ok().and_then(|s| s.parse().ok()))
            .unwrap_or(0);
        metrics
            .http_server_request_size
            .record(request_size, &attributes);

        let fut = self.service.call(req);

        Box::pin(async move {
            let res = fut.await?;
            let (req, res) = res.into_parts();
            metrics.http_server_active_requests.add(-1, &attributes);

            attributes.push(KeyValue::new(
                HTTP_RESPONSE_STATUS_CODE,
                res.status().as_u16() as i64,
            ));

            metrics
                .http_server_request_size
                .record(request_size, &attributes);

            let response_size = match res.body().size() {
                BodySize::Sized(size) => size,
                _ => 0,
            };
            metrics
                .http_server_response_size
                .record(response_size, &attributes);

            let elapsed = timer.elapsed().map(|t| t.as_secs_f64()).unwrap_or_default();
            metrics.http_server_duration.record(elapsed, &attributes);

            Ok(ServiceResponse::new(req, res))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::route;
    use crate::middleware::tracing::record_trace;
    use crate::AppContext;
    use actix_web::middleware::from_fn;
    use actix_web::{test, web, App};
    use opentelemetry::metrics::MeterProvider;
    use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
    use opentelemetry_sdk::testing::metrics::InMemoryMetricsExporter;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_http_metrics() {
        let exporter = InMemoryMetricsExporter::default();
        let meter_provider = SdkMeterProvider::builder()
            .with_reader(
                PeriodicReader::builder(
                    exporter.clone(),
                    opentelemetry_sdk::runtime::TokioCurrentThread,
                ) // runtime::Tokio has an issue to hang
                .build(),
            )
            .build();
        let meter = Arc::new(meter_provider.meter("test"));
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(AppContext::new(meter.clone())))
                .wrap(from_fn(record_trace))
                .wrap(HttpMetrics::new(meter.clone()))
                .configure(route),
        )
        .await;
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        meter_provider.force_flush().unwrap();

        let finished_metrics = exporter.get_finished_metrics().unwrap();
        let finished_metrics_name = finished_metrics
            .iter()
            .flat_map(|resource_metrics| {
                resource_metrics
                    .scope_metrics
                    .iter()
                    .flat_map(|scope_metrics| scope_metrics.metrics.iter().map(|m| m.name.as_ref()))
            })
            .collect::<Vec<_>>();

        assert!(finished_metrics_name.contains(&HTTP_SERVER_ACTIVE_REQUESTS));
        assert!(finished_metrics_name.contains(&HTTP_SERVER_DURATION));
        assert!(finished_metrics_name.contains(&HTTP_SERVER_REQUEST_SIZE));
        assert!(finished_metrics_name.contains(&HTTP_SERVER_RESPONSE_SIZE));
    }
}
