# Kleio

> [Kleio](https://en.wikipedia.org/wiki/Clio) is the Roman Goddess of History. This crate is meant to unify telemetry and tracing groundwork for rust crates.

## Telemetry

In `telemetry`, the `Telemetry` struct allows for creating OpenTelemetry providers for traces and logs.

- `Telemetry::default`: create a new instance of `Telemetry` without any layers. This does not initialise anything yet.
- `Telemetry::initialize`: register the included layers. Use one of the functions below to include different layers.

### Layers

Layers are added to capture traces and logs. Each take a `directives: HashMap<String, LevelFilter>` parameter. This defines the filtering directives to use for the given layer. For example:

```
[
  ("h2", LevelFilter::ERROR),
  ("tower_http", LevelFilter::ERROR)
]
```

The functions to include layers are as follows:

- `Telemetry::include_otel_tracing_layer`
- `Telemetry::include_stdout_layer`
- `Telemetry::include_tracing_bridge_layer`

### Parent Context Propagation

In `http`, a `Tracing` struct is defined that can be used to build a tower::Layer to create a root request span and attach parent context to it. It can then be used in an axum layer.

- `Tracing::new`: create a new instance with a `service_name`.
- `Tracing::set_tracing_level`: change the default `INFO` tracing level of response spans to `tracing_level`.
- `Tracing::to_layer`: create a `tower::Layer`.

## Tracing

The `tracing` module provides a small `Tracing` struct that can be used to attach events to the span with relevant attributes. It is meant to be extended with implementations that can make use of `Tracing::add_attribute`.

- `Tracing::add_attribute`: add a key/value pair of attributes to attach to the event.
- `Tracing::add_tracing_event`: attach an event with the stored attributes to the span.

### Extension Implementation Example

```
impl Tracing {
    fn add_ids(&mut self, ids: &Ids) {
        self.add_practitioner(&ids.practitioner_id);
        self.add_organization(&ids.organization_id);
        self.add_patient(&ids.patient_id);
    }
}
```
