# IoT Gateway. REST + gRPC + Streaming

A sensor data pipeline where the same handlers serve both REST
(for dashboards) and gRPC (for IoT devices).

## Why this matters

IoT devices send telemetry over gRPC, binary protobuf is compact
and fast over constrained networks. Web dashboards consume the same
data over REST. JSON is debuggable and works with any HTTP client.

Most frameworks force you to write separate handlers for each protocol.
Typeway serves both from the same code:

```rust
/// Submit a sensor reading.
/// REST: POST /sensors/:id/reading with JSON body
/// gRPC: SubmitSensorReading RPC with protobuf body
async fn submit_reading(
    path: Path<SensorReadingPath>,
    state: State<AppState>,
    body: Json<SensorReading>,
) -> (http::StatusCode, Json<ReadingAck>) {
    // One handler. IoT devices call it via gRPC.
    // Dashboards call it via REST. Same validation,
    // same business logic, same response.
}
```

## The API type

```rust
type SensorAPI = (
    GetEndpoint<SensorsPath, Vec<Sensor>>,
    GetEndpoint<SensorReadingPath, ReadingAck>,
    PostEndpoint<SensorReadingPath, SensorReading, ReadingAck>,
    GetEndpoint<GatewayStatusPath, GatewayStatus>,
    ServerStream<GetEndpoint<SensorFeedPath, Vec<ReadingAck>>>,
);
```

From this one type, typeway generates:
- REST server (JSON on `/sensors`, `/sensors/:id/reading`, etc.)
- gRPC server (`iot.v1.SensorService` with all RPCs)
- `.proto` file (printed at startup)
- gRPC service documentation (`GET /grpc-docs`)
- Server reflection (grpcurl can discover the API)
- Health check endpoint

## Run

```bash
cargo run -p typeway-iot-gateway
```

## Test as REST (dashboard)

```bash
# List sensors
curl http://localhost:3000/sensors

# Submit a reading (as a dashboard/admin tool would)
curl -X POST http://localhost:3000/sensors/1/reading \
  -H 'Content-Type: application/json' \
  -d '{"temperature": 22.5, "humidity": 65.0, "battery_pct": 87}'

# Get latest reading
curl http://localhost:3000/sensors/1/reading

# Gateway status
curl http://localhost:3000/gateway/status

# View gRPC docs
open http://localhost:3000/grpc-docs
```

## Test as gRPC (IoT device)

```bash
# Discover services
grpcurl -plaintext localhost:3000 list

# Submit a reading (as an IoT device would)
grpcurl -plaintext \
  -d '{"temperature": 22.5, "humidity": 65.0, "battery_pct": 87}' \
  localhost:3000 iot.v1.SensorService/SubmitSensorReading

# Get sensor feed (server-streaming)
grpcurl -plaintext localhost:3000 iot.v1.SensorService/ListSensorFeed
```

## Streaming

The sensor feed endpoint (`ServerStream<GetEndpoint<...>>`) works
differently for each protocol:

- **REST**: Returns a JSON array of all readings
- **gRPC**: Streams individual readings as separate gRPC frames

Same handler, same data, different wire format.

## Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Gateway server with dual-protocol handlers |
