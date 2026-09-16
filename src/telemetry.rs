use std::collections::HashMap;
use std::str::FromStr;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::Directive;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, Layer, Registry, fmt};

pub struct Telemetry {
    tracer_provider: SdkTracerProvider,
    logger_provider: SdkLoggerProvider,
    layers: Vec<Box<dyn Layer<Registry> + Send + Sync>>,
}

fn create_tracer_provider() -> SdkTracerProvider {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .build()
        .expect("building OTel gRPC span exporter should not fail");

    let provider = SdkTracerProvider::builder().with_batch_exporter(exporter);

    provider.build()
}

fn create_logger_provider() -> SdkLoggerProvider {
    let exporter = opentelemetry_otlp::LogExporter::builder()
        .with_http()
        .build()
        .expect("building OTel log exporter should not fail");

    let provider = SdkLoggerProvider::builder().with_batch_exporter(exporter);

    provider.build()
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            tracer_provider: create_tracer_provider(),
            logger_provider: create_logger_provider(),
            layers: vec![],
        }
    }
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

impl Telemetry {
    pub fn include_otel_tracing_layer(&mut self, directives: &HashMap<String, LevelFilter>) {
        // let no_span_event_filter =
        //     tracing_subscriber::filter::filter_fn(|metadata| !metadata.is_event());

        let otel_tracing_layer = tracing_opentelemetry::layer()
            .with_tracer(self.tracer_provider.tracer("otel-subscriber"))
            .with_filter(create_otel_filter(directives)) //.and(no_span_event_filter))
            .boxed();

        self.layers.push(otel_tracing_layer);
    }

    pub fn include_stdout_layer(&mut self, directives: &HashMap<String, LevelFilter>) {
        let stdout_layer = create_stdout_layer::<Registry>()
            .pretty()
            .with_filter(create_otel_filter(directives))
            .boxed();

        self.layers.push(stdout_layer);
    }

    pub fn include_tracing_bridge_layer(&mut self, directives: &HashMap<String, LevelFilter>) {
        let tracing_bridge_layer = OpenTelemetryTracingBridge::new(&self.logger_provider)
            .with_filter(create_otel_filter(directives))
            .boxed();

        self.layers.push(tracing_bridge_layer);
    }

    pub fn initialize(self) -> Guard {
        let Telemetry {
            tracer_provider,
            logger_provider,
            layers,
        } = self;

        tracing_subscriber::registry().with(layers).init();

        Guard {
            tracer_provider,
            logger_provider,
        }
    }
}

pub struct Guard {
    tracer_provider: SdkTracerProvider,
    logger_provider: SdkLoggerProvider,
}

impl Guard {
    pub fn flush(&self) {
        if let Err(err) = self.tracer_provider.force_flush() {
            eprintln!("Failed to force flush the trace provider: {err:?}");
        }

        if let Err(err) = self.logger_provider.force_flush() {
            eprintln!("Failed to force flush the log provider: {err:?}");
        }
    }
}

impl Drop for Guard {
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
