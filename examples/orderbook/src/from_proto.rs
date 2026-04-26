//! # Proto-First Order Book — same service, generated from `.proto`
//!
//! This is the **same trading service** as `main.rs`, but the types are
//! generated from a `.proto` definition instead of hand-written.
//!
//! Compare the two approaches:
//!
//! | | `main.rs` (Rust-first) | `from_proto.rs` (Proto-first) |
//! |---|---|---|
//! | Source of truth | Rust types | `.proto` file |
//! | String type | `BytesStr` (manual) | `BytesStr` (automatic) |
//! | Codec derive | `#[derive(TypewayCodec)]` (manual) | `#[derive(TypewayCodec)]` (generated) |
//! | ToProtoType impls | Hand-written | `#[derive(ToProtoType)]` |
//! | Performance | Identical | Identical |
//!
//! ## Run
//!
//! ```bash
//! cargo run -p typeway-orderbook --bin typeway-orderbook-from-proto
//! ```
//!
//! ## Test (same as main.rs)
//!
//! ```bash
//! grpcurl -plaintext localhost:3000 list
//! grpcurl -plaintext -d '{"symbol":"AAPL","side":"buy","price":185.50,"quantity":100}' \
//!   localhost:3000 trading.v1.OrderBook/SubmitOrder
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};

use typeway_core::endpoint::*;
use typeway_grpc::streaming::ServerStream;
use typeway_macros::{ToProtoType, TypewayCodec};
use typeway_protobuf::BytesStr;
use typeway_server::*;

// =========================================================================
// Step 1: The .proto definition (your team's contract)
// =========================================================================

const TRADING_PROTO: &str = r#"syntax = "proto3";

package trading.v1;

service OrderBook {
  // POST /orders
  rpc SubmitOrder(Order) returns (OrderAck);
  // POST /orders/cancel
  rpc CancelOrder(CancelRequest) returns (CancelAck);
  // POST /book
  rpc GetOrderBook(SymbolQuery) returns (OrderBookSnapshot);
  // GET /prices
  rpc StreamPrices(google.protobuf.Empty) returns (stream PriceUpdate);
}

message Order {
  string symbol = 1;
  string side = 2;
  double price = 3;
  uint32 quantity = 4;
}

message OrderAck {
  string order_id = 1;
  string symbol = 2;
  string side = 3;
  double price = 4;
  uint32 quantity = 5;
  string status = 6;
  uint64 timestamp_ns = 7;
}

message CancelRequest {
  string order_id = 1;
}

message CancelAck {
  string order_id = 1;
  string status = 2;
}

message SymbolQuery {
  string symbol = 1;
}

message OrderBookSnapshot {
  string symbol = 1;
  repeated PriceLevel bids = 2;
  repeated PriceLevel asks = 3;
  double last_trade_price = 4;
  uint64 volume = 5;
}

message PriceLevel {
  double price = 1;
  uint32 quantity = 2;
  uint32 order_count = 3;
}

message PriceUpdate {
  string symbol = 1;
  double bid = 2;
  double ask = 3;
  double last = 4;
  uint64 volume = 5;
  uint64 timestamp_ns = 6;
}
"#;

// =========================================================================
// Step 2: Generated types
//
// In a real project, you'd run this in build.rs:
//
//   let code = typeway_grpc::proto_to_typeway_with_codec(TRADING_PROTO)?;
//   std::fs::write("src/generated.rs", code)?;
//
// Then `include!("generated.rs")` or `mod generated;`
//
// Here we write them out so you can see exactly what the codegen produces.
// Every struct below matches the codegen output — BytesStr for strings,
// TypewayCodec derive, proto tags on every field.
// =========================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, ToProtoType)]
struct Order {
    #[proto(tag = 1)]
    symbol: BytesStr,
    #[proto(tag = 2)]
    side: BytesStr,
    #[proto(tag = 3)]
    price: f64,
    #[proto(tag = 4)]
    quantity: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, ToProtoType)]
struct OrderAck {
    #[proto(tag = 1)]
    order_id: BytesStr,
    #[proto(tag = 2)]
    symbol: BytesStr,
    #[proto(tag = 3)]
    side: BytesStr,
    #[proto(tag = 4)]
    price: f64,
    #[proto(tag = 5)]
    quantity: u32,
    #[proto(tag = 6)]
    status: BytesStr,
    #[proto(tag = 7)]
    timestamp_ns: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, ToProtoType)]
struct CancelRequest {
    #[proto(tag = 1)]
    order_id: BytesStr,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, ToProtoType)]
struct CancelAck {
    #[proto(tag = 1)]
    order_id: BytesStr,
    #[proto(tag = 2)]
    status: BytesStr,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, ToProtoType)]
struct SymbolQuery {
    #[proto(tag = 1)]
    symbol: BytesStr,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, ToProtoType)]
struct OrderBookSnapshot {
    #[proto(tag = 1)]
    symbol: BytesStr,
    #[proto(tag = 2)]
    bids: Vec<PriceLevel>,
    #[proto(tag = 3)]
    asks: Vec<PriceLevel>,
    #[proto(tag = 4)]
    last_trade_price: f64,
    #[proto(tag = 5)]
    volume: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, ToProtoType)]
struct PriceLevel {
    #[proto(tag = 1)]
    price: f64,
    #[proto(tag = 2)]
    quantity: u32,
    #[proto(tag = 3)]
    order_count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToProtoType)]
struct PriceUpdate {
    #[proto(tag = 1)]
    symbol: String,
    #[proto(tag = 2)]
    bid: f64,
    #[proto(tag = 3)]
    ask: f64,
    #[proto(tag = 4)]
    last: f64,
    #[proto(tag = 5)]
    volume: u64,
    #[proto(tag = 6)]
    timestamp_ns: u64,
}

// =========================================================================
// Step 3: Path types and API (also generated by codegen)
// =========================================================================

typeway_macros::typeway_path!(type OrdersPath = "orders");
typeway_macros::typeway_path!(type CancelPath = "orders" / "cancel");
typeway_macros::typeway_path!(type BookPath = "book");
typeway_macros::typeway_path!(type PricesPath = "prices");

type OrderBookAPI = (
    PostEndpoint<OrdersPath, Order, OrderAck>,
    PostEndpoint<CancelPath, CancelRequest, CancelAck>,
    PostEndpoint<BookPath, SymbolQuery, OrderBookSnapshot>,
    ServerStream<GetEndpoint<PricesPath, Vec<PriceUpdate>>>,
);

// =========================================================================
// Step 4: Handlers (identical to main.rs — the business logic doesn't change)
// =========================================================================

#[derive(Clone)]
struct Exchange {
    orders: Arc<Mutex<Vec<OrderAck>>>,
    next_id: Arc<AtomicU64>,
    price_tx: broadcast::Sender<PriceUpdate>,
}

impl Exchange {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Exchange {
            orders: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(AtomicU64::new(1)),
            price_tx: tx,
        }
    }

    fn next_order_id(&self) -> String {
        format!("ORD-{:06}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn timestamp_ns() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }
}

async fn submit_order(
    state: State<Exchange>,
    body: Json<Order>,
) -> (http::StatusCode, Json<OrderAck>) {
    let ack = OrderAck {
        order_id: BytesStr::from(state.0.next_order_id()),
        symbol: body.0.symbol.clone(),
        side: body.0.side.clone(),
        price: body.0.price,
        quantity: body.0.quantity,
        status: BytesStr::from("accepted"),
        timestamp_ns: Exchange::timestamp_ns(),
    };

    let _ = state.0.price_tx.send(PriceUpdate {
        symbol: ack.symbol.to_string(),
        bid: ack.price - 0.05,
        ask: ack.price + 0.05,
        last: ack.price,
        volume: ack.quantity as u64,
        timestamp_ns: ack.timestamp_ns,
    });

    state.0.orders.lock().await.push(ack.clone());
    (http::StatusCode::CREATED, Json(ack))
}

async fn cancel_order(state: State<Exchange>, body: Json<CancelRequest>) -> Json<CancelAck> {
    let mut orders = state.0.orders.lock().await;
    let status = if let Some(order) = orders.iter_mut().find(|o| *o.order_id == *body.0.order_id) {
        order.status = BytesStr::from("cancelled");
        "cancelled"
    } else {
        "not_found"
    };

    Json(CancelAck {
        order_id: body.0.order_id.clone(),
        status: BytesStr::from(status),
    })
}

async fn get_order_book(
    state: State<Exchange>,
    body: Json<SymbolQuery>,
) -> Json<OrderBookSnapshot> {
    let orders = state.0.orders.lock().await;
    let symbol_orders: Vec<_> = orders
        .iter()
        .filter(|o| *o.symbol == *body.0.symbol && *o.status == *"accepted")
        .collect();

    let mut bids: Vec<PriceLevel> = Vec::new();
    let mut asks: Vec<PriceLevel> = Vec::new();

    for order in &symbol_orders {
        let level = PriceLevel {
            price: order.price,
            quantity: order.quantity,
            order_count: 1,
        };
        if &*order.side == "buy" {
            bids.push(level);
        } else {
            asks.push(level);
        }
    }

    bids.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());
    asks.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());

    Json(OrderBookSnapshot {
        symbol: body.0.symbol.clone(),
        bids,
        asks,
        last_trade_price: symbol_orders.last().map(|o| o.price).unwrap_or(0.0),
        volume: symbol_orders.iter().map(|o| o.quantity as u64).sum(),
    })
}

async fn stream_prices(state: State<Exchange>) -> Json<Vec<PriceUpdate>> {
    let mut rx = state.0.price_tx.subscribe();
    let mut updates = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    while updates.len() < 10 {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(update)) => updates.push(update),
            _ => break,
        }
    }
    Json(updates)
}

// =========================================================================
// Step 5: Start the server — show the codegen output first
// =========================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    // Show what the codegen produces from the proto definition.
    let generated =
        typeway_grpc::proto_to_typeway_with_codec(TRADING_PROTO).expect("proto parse failed");
    tracing::info!("=== What proto_to_typeway_with_codec() generates ===");
    for line in generated.lines() {
        tracing::info!("  {line}");
    }
    tracing::info!("=== End generated code ===\n");

    let exchange = Exchange::new();

    tracing::info!("Starting proto-first order book on http://localhost:3000");
    tracing::info!("  Same API as main.rs — types generated from .proto");

    Server::<OrderBookAPI>::new((
        bind::<_, _, _>(submit_order),
        bind::<_, _, _>(cancel_order),
        bind::<_, _, _>(get_order_book),
        bind::<_, _, _>(stream_prices),
    ))
    .with_state(exchange)
    .with_grpc("OrderBook", "trading.v1")
    .serve("0.0.0.0:3000".parse()?)
    .await
}
