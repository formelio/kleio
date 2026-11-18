use std::collections::HashMap;
use std::str::FromStr;

use opentelemetry::Value;
use opentelemetry::propagation::TextMapCompositePropagator;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::ExporterBuildError;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::propagation::{BaggagePropagator, TraceContextPropagator};
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::Directive;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::{SubscriberInitExt as _, TryInitError};
use tracing_subscriber::{EnvFilter, Layer as TracingLayer, Registry, fmt};

pub type Layer = Box<dyn TracingLayer<Registry> + Send + Sync>;

fn propagator() -> TextMapCompositePropagator {
    TextMapCompositePropagator::new(vec![
        Box::new(TraceContextPropagator::new()),
        Box::new(BaggagePropagator::new()),
    ])
}

pub struct Builder {
    tracer_provider: SdkTracerProvider,
    logger_provider: SdkLoggerProvider,
    layers: Vec<Layer>,
}

fn create_tracer_provider(resource: Resource) -> Result<SdkTracerProvider, ExporterBuildError> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()?;

    let provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter);

    Ok(provider.build())
}

fn create_logger_provider(resource: Resource) -> Result<SdkLoggerProvider, ExporterBuildError> {
    let exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .build()?;

    let provider = SdkLoggerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter);

    Ok(provider.build())
}

fn create_stdout_layer<T>() -> fmt::Layer<T> {
    fmt::Layer::new()
        .with_ansi(true)
        .with_file(true)
        .with_line_number(true)
        .with_target(true)
        .with_writer(std::io::stdout)
}

fn new_directive(target: &str, level: LevelFilter) -> Directive {
    Directive::from_str(&format!("{target}={level}")).unwrap()
}

/// `directives` are used only when `RUST_LOG` is missing.
fn create_otel_filter(directives: &HashMap<String, LevelFilter>) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let env_filter = EnvFilter::builder()
            .with_default_directive(LevelFilter::INFO.into())
            .parse_lossy("");

        directives
            .iter()
            .fold(env_filter, |env_filter, (directive, level)| {
                env_filter.add_directive(new_directive(directive, *level))
            })
    })
}

impl Builder {
    pub fn new(service_name: impl Into<Value>) -> Result<Self, ExporterBuildError> {
        let resource = Resource::builder().with_service_name(service_name).build();

        Ok(Self {
            tracer_provider: create_tracer_provider(resource.clone())?,
            logger_provider: create_logger_provider(resource)?,
            layers: vec![],
        })
    }

    pub fn add_layer(&mut self, layer: Layer) {
        self.layers.push(layer);
    }

    pub fn create_tracing_layer(&self, directives: &HashMap<String, LevelFilter>) -> Layer {
        tracing_opentelemetry::layer()
            .with_tracer(self.tracer_provider.tracer("otel-subscriber"))
            .with_filter(create_otel_filter(directives))
            .boxed()
    }

    pub fn with_tracing_layer(&mut self, directives: &HashMap<String, LevelFilter>) {
        self.add_layer(self.create_tracing_layer(directives));
    }

    pub fn create_stdout_layer(&self, directives: &HashMap<String, LevelFilter>) -> Layer {
        create_stdout_layer::<Registry>()
            .pretty()
            .with_filter(create_otel_filter(directives))
            .boxed()
    }

    pub fn with_stdout_layer(&mut self, directives: &HashMap<String, LevelFilter>) {
        self.add_layer(self.create_stdout_layer(directives));
    }

    pub fn create_tracing_bridge_layer(&self, directives: &HashMap<String, LevelFilter>) -> Layer {
        OpenTelemetryTracingBridge::new(&self.logger_provider)
            .with_filter(create_otel_filter(directives))
            .boxed()
    }

    pub fn with_tracing_bridge_layer(&mut self, directives: &HashMap<String, LevelFilter>) {
        self.add_layer(self.create_tracing_bridge_layer(directives));
    }

    pub fn initialize(self) -> Result<Telemetry, TryInitError> {
        let Builder {
            tracer_provider,
            logger_provider,
            layers,
        } = self;

        tracing_subscriber::registry().with(layers).try_init()?;
        opentelemetry::global::set_tracer_provider(tracer_provider.clone());
        opentelemetry::global::set_text_map_propagator(propagator());

        Ok(Telemetry {
            tracer_provider,
            logger_provider,
        })
    }
}

pub struct Telemetry {
    tracer_provider: SdkTracerProvider,
    logger_provider: SdkLoggerProvider,
}

impl Telemetry {
    pub fn flush(&self) {
        if let Err(err) = self.tracer_provider.force_flush() {
            eprintln!("Failed to force flush the trace provider: {err:?}");
        }

        if let Err(err) = self.logger_provider.force_flush() {
            eprintln!("Failed to force flush the log provider: {err:?}");
        }
    }
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        self.flush();

        if let Err(error) = self.tracer_provider.shutdown() {
            eprintln!("failed to shutdown otel tracer provider: {error:?}");
        }

        if let Err(error) = self.logger_provider.shutdown() {
            eprintln!("failed to shutdown otel logger provider: {error:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("tower_http", LevelFilter::ERROR, "tower_http=error")]
    #[case("h2", LevelFilter::WARN, "h2=warn")]
    #[case("kleio::http", LevelFilter::DEBUG, "kleio::http=debug")]
    #[case("hyper", LevelFilter::TRACE, "hyper=trace")]
    #[case("noisy_crate", LevelFilter::OFF, "noisy_crate=off")]
    fn new_directive_formats_target_and_level(
        #[case] target: &str,
        #[case] level: LevelFilter,
        #[case] expected: &str,
    ) {
        assert_eq!(new_directive(target, level).to_string(), expected);
    }

    #[derive(Default)]
    struct Headers(HashMap<String, String>);

    impl opentelemetry::propagation::Injector for Headers {
        fn set(&mut self, key: &str, value: String) {
            self.0.insert(key.to_string(), value);
        }
    }

    fn inject(context: &opentelemetry::Context) -> HashMap<String, String> {
        use opentelemetry::propagation::TextMapPropagator as _;

        let mut headers = Headers::default();
        propagator().inject_context(context, &mut headers);

        headers.0
    }

    fn sampled_context() -> opentelemetry::Context {
        use opentelemetry::trace::{
            SpanContext, SpanId, TraceContextExt as _, TraceFlags, TraceId, TraceState,
        };

        opentelemetry::Context::new().with_remote_span_context(SpanContext::new(
            TraceId::from_hex("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
            SpanId::from_hex("00f067aa0ba902b7").unwrap(),
            TraceFlags::SAMPLED,
            true,
            TraceState::default(),
        ))
    }

    #[test]
    fn omits_the_baggage_header_when_no_baggage_is_set() {
        let headers = inject(&sampled_context());

        assert!(headers.contains_key("traceparent"));
        assert!(!headers.contains_key("baggage"));
    }

    #[test]
    fn injects_baggage_alongside_the_trace_context() {
        use opentelemetry::KeyValue;
        use opentelemetry::baggage::BaggageExt as _;

        let context =
            sampled_context().with_baggage([KeyValue::new("audit.event.id", "event-1".to_string())]);

        let headers = inject(&context);

        assert!(headers.contains_key("traceparent"));
        assert_eq!(headers.get("baggage").map(String::as_str), Some("audit.event.id=event-1"));
    }
}
