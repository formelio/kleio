# Kleio

> [Kleio](https://en.wikipedia.org/wiki/Clio) is the Roman Goddess of History. This crate is meant to unify telemetry and tracing groundwork for rust crates.

## Telemetry

In `telemetry`, the `Builder` struct allows for creating OpenTelemetry providers for traces and logs.

- `Builder::new`: create a new builder without any layers, for the given `service_name`. This does not initialise anything yet. It returns an `Err` when an OTLP exporter cannot be built.
- `Builder::initialize`: register the added layers and return a `Telemetry` handle. Use one of the functions below to add layers. It returns an `Err` when a global default subscriber is already set.
- `Telemetry::flush`: force flush the trace and log providers. This also happens on drop, together with shutting both providers down.

The `service_name` is attached as the `service.name` resource attribute on every exported span and log record, which is how backends group telemetry per service. It takes precedence over the `OTEL_SERVICE_NAME` environment variable.

### Layers

Layers are added to capture traces and logs. Each takes a `directives: &HashMap<String, LevelFilter>` parameter. This defines the filtering directives to use for the given layer. For example:

```
[
  ("h2", LevelFilter::ERROR),
  ("tower_http", LevelFilter::ERROR)
]
```

These directives are only a fallback: when `RUST_LOG` is set, it is used verbatim and the passed directives are ignored.

The functions to add layers are as follows:

- `Builder::with_tracing_layer`: export spans over OTLP.
- `Builder::with_stdout_layer`: pretty-print to stdout.
- `Builder::with_tracing_bridge_layer`: export `tracing` events as OTLP log records.

Each has a `create_*_layer` counterpart returning the boxed `Layer` instead of adding it, for use with `Builder::add_layer`.

### Parent Context Propagation

In `http`, a `Tracing` struct is defined that can be used to build a tower::Layer to create a root request span and attach parent context to it. It can then be used in an axum layer.

- `Tracing::new`: create a new instance with a `service_name`.
- `Tracing::to_layer`: create a `tower::Layer` with the given tracing level for response tracing.
