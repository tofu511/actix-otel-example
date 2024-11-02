use crate::middleware::tracing::TraceInfo;
use crate::AppContext;
use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use opentelemetry::KeyValue;
use opentelemetry_semantic_conventions::attribute::HTTP_REQUEST_METHOD;
use tracing::{instrument, Span};

#[get("/")]
pub async fn hello(trace_info: web::ReqData<TraceInfo>) -> impl Responder {
    foo(trace_info.into_inner()).await;
    HttpResponse::Ok().body("Hello world!")
}

#[post("/echo")]
pub async fn echo(
    req: HttpRequest,
    req_body: String,
    trace_info: web::ReqData<TraceInfo>,
) -> impl Responder {
    tracing::event!(
        tracing::Level::INFO,
        { HTTP_REQUEST_METHOD } = req.method().as_str(),
    );
    foo(trace_info.into_inner()).await;
    HttpResponse::Ok().body(req_body)
}

#[post("/metrics")]
pub async fn metrics(context: web::Data<AppContext>) -> impl Responder {
    let counter = context.meter.f64_counter("ops_count").init();
    counter.add(1.0, &[KeyValue::new("my-key", "my-value")]);
    HttpResponse::Ok()
}

#[instrument(parent = _trace_info.parent_span.clone())]
async fn foo(_trace_info: TraceInfo) {
    tracing::info_span!("this is inside the foo func");
}
