use opentelemetry::propagation::TextMapPropagator as _;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tower_http::{
    classify::{ServerErrorsAsFailures, SharedClassifier},
    trace::TraceLayer,
};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

#[derive(Clone)]
pub struct Tracing {
    propagator: TraceContextPropagator,
    tracing_level: tracing::Level,
    service_name: String,
}

impl Tracing {
    pub fn new(service_name: String) -> Self {
        Self {
            propagator: TraceContextPropagator::new(),
            tracing_level: tracing::Level::INFO,
            service_name,
        }
    }

    pub fn set_tracing_level(&mut self, tracing_level: tracing::Level) {
        self.tracing_level = tracing_level;
    }

    pub fn to_layer(self) -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>, Self> {
        tower_http::trace::TraceLayer::new_for_http()
            .on_response(tower_http::trace::DefaultOnResponse::new().level(self.tracing_level))
            .make_span_with(self)
    }
}

impl<B> tower_http::trace::MakeSpan<B> for Tracing {
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
