//! # IoT Gateway — REST + gRPC + Streaming from the Same Handlers
//!
//! A sensor data pipeline where:
//!
//! - **REST** serves dashboards and admin tools (JSON)
//! - **gRPC** handles device-to-server telemetry (binary protobuf)
//! - **Server streaming** pushes live sensor feeds to subscribers
//! - **The same handlers serve both protocols** — zero duplication
//!
//! This demonstrates the dual-protocol story with a real use case
//! where it matters: IoT devices speak gRPC for efficiency, while
//! web dashboards consume the same data via REST.
//!
//! ## Run
//!
//! ```bash
//! cargo run -p typeway-iot-gateway
//! ```
//!
//! ## Test
//!
//! ```bash
//! # REST: get all sensors
//! curl http://localhost:3000/sensors
//!
//! # REST: get latest reading for sensor 1
//! curl http://localhost:3000/sensors/1/reading
//!
//! # REST: submit a reading (same as gRPC, different wire format)
//! curl -X POST http://localhost:3000/sensors/1/reading \
//!   -H 'Content-Type: application/json' \
//!   -d '{"temperature": 22.5, "humidity": 65.0, "battery_pct": 87}'
//!
//! # gRPC: submit a reading (binary protobuf)
//! grpcurl -plaintext -d '{"temperature": 22.5, "humidity": 65.0, "battery_pct": 87}' \
//!   localhost:3000 iot.v1.SensorService/SubmitSensorReading
//!
//! # gRPC: list services
//! grpcurl -plaintext localhost:3000 list
//!
//! # REST: gateway status
//! curl http://localhost:3000/gateway/status
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use typeway_core::endpoint::*;
use typeway_core::path::{Capture, HCons, HNil, Lit, LitSegment};
use typeway_grpc::streaming::ServerStream;
use typeway_macros::ToProtoType;
use typeway_server::*;

// =========================================================================
// Path types
// =========================================================================

#[allow(non_camel_case_types)]
struct __lit_sensors;
impl LitSegment for __lit_sensors {
    const VALUE: &'static str = "sensors";
}

#[allow(non_camel_case_types)]
struct __lit_reading;
impl LitSegment for __lit_reading {
    const VALUE: &'static str = "reading";
}

#[allow(non_camel_case_types)]
struct __lit_gateway;
impl LitSegment for __lit_gateway {
    const VALUE: &'static str = "gateway";
}

#[allow(non_camel_case_types)]
struct __lit_status;
impl LitSegment for __lit_status {
    const VALUE: &'static str = "status";
}

#[allow(non_camel_case_types)]
struct __lit_feed;
impl LitSegment for __lit_feed {
    const VALUE: &'static str = "feed";
}

type SensorsPath = HCons<Lit<__lit_sensors>, HNil>;
type SensorReadingPath = HCons<Lit<__lit_sensors>, HCons<Capture<u32>, HCons<Lit<__lit_reading>, HNil>>>;
type GatewayStatusPath = HCons<Lit<__lit_gateway>, HCons<Lit<__lit_status>, HNil>>;
type SensorFeedPath = HCons<Lit<__lit_sensors>, HCons<Lit<__lit_feed>, HNil>>;

// =========================================================================
// Domain types — same types for REST and gRPC
// =========================================================================

/// A registered sensor device.
#[derive(Debug, Clone, Serialize, Deserialize, ToProtoType)]
struct Sensor {
    id: u32,
    name: String,
    location: String,
    online: bool,
}

/// A sensor reading — submitted by devices, consumed by dashboards.
#[derive(Debug, Clone, Serialize, Deserialize, ToProtoType)]
struct SensorReading {
    temperature: f64,
    humidity: f64,
    battery_pct: u32,
}

/// Acknowledged reading with timestamp.
#[derive(Debug, Clone, Serialize, Deserialize, ToProtoType)]
struct ReadingAck {
    sensor_id: u32,
    reading_number: u64,
    temperature: f64,
    humidity: f64,
    battery_pct: u32,
    timestamp: String,
}

/// Gateway status — aggregate stats.
#[derive(Debug, Clone, Serialize, ToProtoType)]
struct GatewayStatus {
    sensors_online: usize,
    total_readings: u64,
    version: String,
}

// =========================================================================
// Shared state
// =========================================================================

#[derive(Clone)]
struct AppState {
    sensors: Arc<Mutex<Vec<Sensor>>>,
    readings: Arc<Mutex<Vec<ReadingAck>>>,
    reading_count: Arc<AtomicU64>,
}

impl AppState {
    fn new() -> Self {
        let sensors = vec![
            Sensor { id: 1, name: "Living Room".into(), location: "Floor 1".into(), online: true },
            Sensor { id: 2, name: "Greenhouse".into(), location: "Garden".into(), online: true },
            Sensor { id: 3, name: "Server Room".into(), location: "Basement".into(), online: false },
        ];

        AppState {
            sensors: Arc::new(Mutex::new(sensors)),
            readings: Arc::new(Mutex::new(Vec::new())),
            reading_count: Arc::new(AtomicU64::new(0)),
        }
    }
}

// =========================================================================
// API type — one type drives REST, gRPC, proto generation, and docs
// =========================================================================

/// The API is a TYPE. REST and gRPC are projections of this type.
/// The handlers serve both protocols from the same code.
type SensorAPI = (
    // GET /sensors — list all sensors (REST: JSON array, gRPC: unary)
    GetEndpoint<SensorsPath, Vec<Sensor>>,

    // GET /sensors/:id/reading — latest reading for a sensor
    GetEndpoint<SensorReadingPath, ReadingAck>,

    // POST /sensors/:id/reading — submit a reading
    // REST: POST with JSON body
    // gRPC: SubmitSensorReading RPC with protobuf body
    PostEndpoint<SensorReadingPath, SensorReading, ReadingAck>,

    // GET /gateway/status — gateway aggregate stats
    GetEndpoint<GatewayStatusPath, GatewayStatus>,

    // GET /sensors/feed — streaming sensor feed (gRPC server-streaming)
    // REST: returns JSON array of recent readings
    // gRPC: streams individual readings as frames
    ServerStream<GetEndpoint<SensorFeedPath, Vec<ReadingAck>>>,
);

// =========================================================================
// Handlers — SAME handlers for REST and gRPC
// =========================================================================

/// List all sensors.
/// REST: GET /sensors → JSON array
/// gRPC: ListSensor RPC → repeated Sensor
async fn list_sensors(state: State<AppState>) -> Json<Vec<Sensor>> {
    Json(state.0.sensors.lock().await.clone())
}

/// Get the latest reading for a sensor.
/// REST: GET /sensors/:id/reading → JSON
/// gRPC: GetSensorReading RPC → ReadingAck
async fn get_reading(
    path: Path<SensorReadingPath>,
    state: State<AppState>,
) -> Result<Json<ReadingAck>, http::StatusCode> {
    let (sensor_id,) = path.0;
    let readings = state.0.readings.lock().await;
    readings
        .iter()
        .rev()
        .find(|r| r.sensor_id == sensor_id)
        .cloned()
        .map(Json)
        .ok_or(http::StatusCode::NOT_FOUND)
}

/// Submit a sensor reading.
/// REST: POST /sensors/:id/reading with JSON body
/// gRPC: SubmitSensorReading RPC with protobuf body
///
/// This is the key handler — IoT devices call it via gRPC (binary
/// protobuf, efficient), while admin tools call it via REST (JSON,
/// human-readable). Same handler, same validation, same business logic.
async fn submit_reading(
    path: Path<SensorReadingPath>,
    state: State<AppState>,
    body: Json<SensorReading>,
) -> (http::StatusCode, Json<ReadingAck>) {
    let (sensor_id,) = path.0;
    let reading_num = state.0.reading_count.fetch_add(1, Ordering::Relaxed) + 1;

    let ack = ReadingAck {
        sensor_id,
        reading_number: reading_num,
        temperature: body.0.temperature,
        humidity: body.0.humidity,
        battery_pct: body.0.battery_pct,
        timestamp: chrono_now(),
    };

    tracing::info!(
        "Sensor {sensor_id}: temp={:.1}°C humidity={:.0}% battery={}% (reading #{})",
        ack.temperature, ack.humidity, ack.battery_pct, reading_num
    );

    state.0.readings.lock().await.push(ack.clone());

    (http::StatusCode::CREATED, Json(ack))
}

/// Gateway status.
async fn gateway_status(state: State<AppState>) -> Json<GatewayStatus> {
    let sensors = state.0.sensors.lock().await;
    let online = sensors.iter().filter(|s| s.online).count();
    Json(GatewayStatus {
        sensors_online: online,
        total_readings: state.0.reading_count.load(Ordering::Relaxed),
        version: "1.0.0".into(),
    })
}

/// Sensor feed — returns recent readings.
/// REST: JSON array of all readings
/// gRPC: server-streaming, each reading sent as individual frame
async fn sensor_feed(state: State<AppState>) -> Json<Vec<ReadingAck>> {
    Json(state.0.readings.lock().await.clone())
}

/// Simple timestamp (avoids chrono dependency for this example).
fn chrono_now() -> String {
    format!("2026-03-21T{:02}:{:02}:{:02}Z",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() % 86400 / 3600,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() % 3600 / 60,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() % 60,
    )
}

// =========================================================================
// Main
// =========================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    let state = AppState::new();

    // Print the generated .proto file at startup.
    let proto = <SensorAPI as typeway_grpc::ApiToProto>::to_proto("SensorService", "iot.v1");
    tracing::info!("Generated .proto file:\n{proto}");

    tracing::info!("Starting IoT gateway on http://localhost:3000");
    tracing::info!("  REST:  curl http://localhost:3000/sensors");
    tracing::info!("  gRPC:  grpcurl -plaintext localhost:3000 list");
    tracing::info!("  Both protocols share the same handlers.");

    Server::<SensorAPI>::new((
        bind::<_, _, _>(list_sensors),
        bind::<_, _, _>(get_reading),
        bind::<_, _, _>(submit_reading),
        bind::<_, _, _>(gateway_status),
        bind::<_, _, _>(sensor_feed),
    ))
    .with_state(state)
    .with_grpc("SensorService", "iot.v1")
    .with_grpc_docs()
    .serve("0.0.0.0:3000".parse()?)
    .await
}
