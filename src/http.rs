use opentelemetry::propagation::TextMapPropagator as _;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

#[derive(Clone)]
pub struct ParentContextSpan {
    propagator: TraceContextPropagator,
    service_name: String,
}

impl ParentContextSpan {
    pub fn new(service_name: String) -> Self {
        Self {
            propagator: TraceContextPropagator::new(),
            service_name,
        }
    }
}

impl<B> tower_http::trace::MakeSpan<B> for ParentContextSpan {
    fn make_span(&mut self, request: &axum::http::Request<B>) -> tracing::Span {
        let span = tracing::info_span!(
            "request",
            service.name = self.service_name,
            method = %request.method(),
            uri = %request.uri(),
            version = ?request.version(),
        );

        let parent_context = self
            .propagator
            .extract(&opentelemetry_http::HeaderExtractor(request.headers()));
        if let Err(e) = span.set_parent(parent_context) {
            tracing::error!("Could not set the parent context on the span: {e:?}");
        }

        span
    }
}
