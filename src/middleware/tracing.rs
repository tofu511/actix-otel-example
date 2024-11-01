use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::http::header::{HeaderMap, HeaderName};
use actix_web::middleware::Next;
use actix_web::{Error, HttpMessage, HttpRequest};
use opentelemetry_semantic_conventions::trace::{
    CLIENT_ADDRESS, EXCEPTION_ESCAPED, EXCEPTION_MESSAGE, EXCEPTION_STACKTRACE, EXCEPTION_TYPE,
    HTTP_REQUEST_METHOD, HTTP_RESPONSE_STATUS_CODE, HTTP_ROUTE, NETWORK_PROTOCOL_VERSION, URL_PATH,
    USER_AGENT_ORIGINAL,
};
use tracing::{field, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

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
        "error.type" = empty,
    );
    span.set_parent(opentelemetry::global::get_text_map_propagator(
        |propagator| propagator.extract(&HeaderExtractor(req.headers())),
    ));
    span
}

pub async fn record_trace(
    mut req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let span = make_span(&req);
    req.extensions_mut().insert(span.clone());
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
        span.record("error.type", field::display(res.status()));
    }

    let res = ServiceResponse::new(req, res);

    Ok(res)
}
