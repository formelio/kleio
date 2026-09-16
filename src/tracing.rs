use opentelemetry::KeyValue;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

#[derive(Default)]
pub struct Tracing {
    attributes: Vec<KeyValue>,
}

impl Tracing {
    pub fn add_attribute(&mut self, key: &str, value: &impl ToString) {
        self.attributes
            .push(KeyValue::new(key.to_string(), value.to_string()));
    }

    pub fn add_tracing_event(&self, event: &str) {
        tracing::Span::current().add_event(event.to_string(), self.attributes.clone());
    }
}
