use crate::AppContext;
use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use opentelemetry::KeyValue;
use opentelemetry_semantic_conventions::attribute::HTTP_REQUEST_METHOD;
use tracing::instrument;

#[get("/")]
#[instrument]
pub async fn hello() -> impl Responder {
    foo().await;
    HttpResponse::Ok().body("Hello world!")
}

#[post("/echo")]
#[instrument(skip_all)]
pub async fn echo(req: HttpRequest, req_body: String) -> impl Responder {
    tracing::event!(
        tracing::Level::INFO,
        { HTTP_REQUEST_METHOD } = req.method().as_str(),
    );
    foo().await;
    HttpResponse::Ok().body(req_body)
}

#[post("/metrics")]
#[instrument]
pub async fn metrics(context: web::Data<AppContext>) -> impl Responder {
    let counter = context.meter.f64_counter("ops_count").init();
    counter.add(1.0, &[KeyValue::new("my-key", "my-value")]);
    HttpResponse::Ok()
}

async fn foo() {
    tracing::info_span!("foo");
}
