use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::{InMemorySpanExporterBuilder, SpanData};
use tower::ServiceExt as _;
use tracing_subscriber::layer::SubscriberExt as _;

const TRACE_PARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
const TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

async fn handler() -> axum::http::StatusCode {
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;

    tracing::Span::current().add_event(
        "patient.loaded",
        vec![
            opentelemetry::KeyValue::new("patient.id", 42),
            opentelemetry::KeyValue::new("organization.id", "org-1"),
        ],
    );

    axum::http::StatusCode::NO_CONTENT
}

fn exported_spans_for(trace_parent: Option<&str>) -> Vec<SpanData> {
    let exporter = InMemorySpanExporterBuilder::new().build();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("kleio-test")));

    let app = axum::Router::new()
        .route("/patients/{id}", axum::routing::get(handler))
        .layer(kleio::http::Tracing::new("kleio-test").to_layer(tracing::Level::INFO));

    let mut builder = axum::http::Request::builder().uri("/patients/1");
    if let Some(trace_parent) = trace_parent {
        builder = builder.header("traceparent", trace_parent);
    }
    let request = builder.body(axum::body::Body::empty()).unwrap();

    tracing::subscriber::with_default(subscriber, || {
        let response = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(app.oneshot(request))
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::NO_CONTENT);
    });

    provider.force_flush().unwrap();
    exporter.get_finished_spans().unwrap()
}

fn request_span(trace_parent: Option<&str>) -> SpanData {
    let mut spans = exported_spans_for(trace_parent);

    assert_eq!(spans.len(), 1, "expected exactly one exported request span");
    spans.pop().unwrap()
}

fn attribute(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|kv| kv.key.as_str() == key)
        .map(|kv| kv.value.to_string())
}

#[test]
fn exports_a_request_span_on_the_incoming_trace() {
    let span = request_span(Some(TRACE_PARENT));

    assert_eq!(span.name, "request");
    assert_eq!(span.span_context.trace_id().to_string(), TRACE_ID);
    assert_eq!(span.parent_span_id.to_string(), "00f067aa0ba902b7");
    assert_eq!(
        attribute(&span, "service.name").as_deref(),
        Some("kleio-test")
    );
    assert_eq!(attribute(&span, "method").as_deref(), Some("GET"));
    assert_eq!(attribute(&span, "uri").as_deref(), Some("/patients/1"));
}

#[test]
fn exports_a_root_request_span_without_a_trace_parent_header() {
    let span = request_span(None);

    assert_eq!(span.name, "request");
    assert_ne!(span.span_context.trace_id().to_string(), TRACE_ID);
    assert_eq!(span.parent_span_id, opentelemetry::trace::SpanId::INVALID);
}

#[test]
fn attaches_handler_tracing_events_to_the_request_span() {
    let span = request_span(Some(TRACE_PARENT));

    let event = span
        .events
        .iter()
        .find(|event| event.name == "patient.loaded")
        .expect("handler event should land on the request span");

    let attributes: Vec<_> = event
        .attributes
        .iter()
        .map(|kv| (kv.key.as_str(), kv.value.to_string()))
        .collect();

    assert_eq!(
        attributes,
        vec![
            ("patient.id", "42".to_string()),
            ("organization.id", "org-1".to_string()),
        ]
    );
}
