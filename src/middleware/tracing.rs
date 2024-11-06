use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::http::header::{HeaderMap, HeaderName};
use actix_web::middleware::Next;
use actix_web::{Error, HttpMessage, HttpRequest};
use opentelemetry::propagation::{Extractor, TextMapPropagator};
use opentelemetry::trace::{FutureExt, TraceContextExt, TraceId};
use opentelemetry::Context;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_semantic_conventions::trace::{
    CLIENT_ADDRESS, ERROR_TYPE, EXCEPTION_ESCAPED, EXCEPTION_MESSAGE, EXCEPTION_STACKTRACE,
    EXCEPTION_TYPE, HTTP_REQUEST_METHOD, HTTP_RESPONSE_STATUS_CODE, HTTP_ROUTE,
    NETWORK_PROTOCOL_VERSION, URL_PATH, USER_AGENT_ORIGINAL,
};
use std::any::Any;
use tracing::{field, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

#[derive(Clone, Debug)]
pub struct TraceInfo {
    pub trace_id: TraceId,
    pub app_root_span: Span,
}

impl TraceInfo {
    pub fn new(trace_id: TraceId, app_root_span: Span) -> Self {
        Self {
            trace_id,
            app_root_span,
        }
    }
}

struct HeaderExtractor<'a>(&'a HeaderMap);

impl<'a> opentelemetry::propagation::Extractor for HeaderExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(HeaderName::as_str).collect::<Vec<_>>()
    }
}

fn make_span(req: &ServiceRequest) -> Span {
    let empty = field::Empty;
    let span_name = format!(
        "{} {}",
        req.method(),
        req.match_pattern().unwrap_or_default()
    );
    let span = tracing::info_span!(
        "",
        otel.name = span_name,
        { URL_PATH } = empty,
        { HTTP_ROUTE } = empty,
        { HTTP_REQUEST_METHOD } = empty,
        http.request.headers = empty,
        { HTTP_RESPONSE_STATUS_CODE } = empty,
        { NETWORK_PROTOCOL_VERSION } = empty,
        { CLIENT_ADDRESS } = empty,
        { USER_AGENT_ORIGINAL } = empty,
        { ERROR_TYPE } = empty,
    );
    span.set_parent(opentelemetry::global::get_text_map_propagator(
        |propagator| propagator.extract(&HeaderExtractor(req.headers())),
    ));
    span
}

pub async fn record_trace(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let span = make_span(&req);
    let trace_info = TraceInfo::new(
        span.context().span().span_context().trace_id(),
        span.clone(),
    );
    req.extensions_mut().insert(trace_info);
    let resp = next.call(req).await?;
    let (req, res) = resp.into_parts();

    span.record(URL_PATH, req.path());
    span.record(HTTP_ROUTE, req.match_pattern().unwrap_or_default());
    span.record(HTTP_REQUEST_METHOD, req.method().as_str());
    span.record("http.request.headers", field::debug(req.headers()));
    span.record(NETWORK_PROTOCOL_VERSION, field::debug(req.version()));
    span.record(
        CLIENT_ADDRESS,
        req.connection_info().peer_addr().unwrap_or_default(),
    );

    if let Some(user_agent) = req.headers().get("User-Agent") {
        span.record(USER_AGENT_ORIGINAL, user_agent.to_str().unwrap_or_default());
    }

    span.record(HTTP_RESPONSE_STATUS_CODE, field::display(res.status()));
    if !res.status().is_success() {
        span.record(ERROR_TYPE, field::display(res.status()));
    }

    let res = ServiceResponse::new(req, res);

    Ok(res)
}

#[cfg(test)]
mod tests {
    use crate::api::route;
    use crate::middleware::tracing::record_trace;
    use actix_web::middleware::from_fn;
    use actix_web::{test, App};
    use opentelemetry::global::shutdown_tracer_provider;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::testing::trace::InMemorySpanExporter;
    use opentelemetry_sdk::trace::TracerProvider;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    #[tokio::test]
    async fn test_tracing() {
        let exporter = InMemorySpanExporter::default();
        let provider = TracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();

        let tracer = provider.clone().tracer("test_tracer");
        let trace_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        let _guard = tracing_subscriber::registry()
            .with(trace_layer)
            .set_default();

        let app = test::init_service(App::new().wrap(from_fn(record_trace)).configure(route)).await;
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let spans = exporter.get_finished_spans().unwrap();
        assert!(spans.len() >= 2);

        shutdown_tracer_provider();
    }
}
