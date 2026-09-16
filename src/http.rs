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
    service_name: String,
}

impl Tracing {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            propagator: TraceContextPropagator::new(),
            service_name: service_name.into(),
        }
    }

    pub fn to_layer(
        self,
        tracing_level: tracing::Level,
    ) -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>, Self> {
        tower_http::trace::TraceLayer::new_for_http()
            .on_response(tower_http::trace::DefaultOnResponse::new().level(tracing_level))
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

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::trace::{TraceContextExt as _, TracerProvider as _};
    use tower_http::trace::MakeSpan as _;
    use tracing_subscriber::layer::SubscriberExt as _;

    fn with_subscriber<T>(f: impl FnOnce() -> T) -> T {
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));

        tracing::subscriber::with_default(subscriber, f)
    }

    fn request(trace_parent: Option<&str>) -> axum::http::Request<()> {
        let mut builder = axum::http::Request::builder().uri("/patients/1");
        if let Some(trace_parent) = trace_parent {
            builder = builder.header("traceparent", trace_parent);
        }
        builder.body(()).unwrap()
    }

    fn span_for(trace_parent: Option<&str>) -> opentelemetry::trace::SpanContext {
        with_subscriber(|| {
            let mut tracing = Tracing::new("kleio-test");
            let span = tracing.make_span(&request(trace_parent));

            span.context().span().span_context().clone()
        })
    }

    const TRACE_PARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    const TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

    #[test]
    fn continues_the_trace_from_the_trace_parent_header() {
        let span = span_for(Some(TRACE_PARENT));

        assert_eq!(span.trace_id().to_string(), TRACE_ID);
        assert_ne!(span.span_id().to_string(), "00f067aa0ba902b7");
        assert!(span.is_sampled());
    }

    #[test]
    fn starts_a_new_trace_without_a_trace_parent_header() {
        let span = span_for(None);

        assert!(span.is_valid());
        assert_ne!(span.trace_id().to_string(), TRACE_ID);
    }

    #[test]
    fn starts_a_new_trace_on_a_malformed_trace_parent_header() {
        let span = span_for(Some("not-a-trace-parent"));

        assert!(span.is_valid());
        assert_ne!(span.trace_id().to_string(), TRACE_ID);
    }
}
