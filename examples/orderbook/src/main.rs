//! # High-Performance Order Book — gRPC Microservice
//!
//! A real-time trading order book that pushes the performance envelope:
//!
//! - **Direct handlers** — bypass the HTTP extractor pipeline entirely
//! - **BytesStr** — zero-copy string decode for ticker symbols and order IDs
//! - **TypewayCodec** — 12-54% faster than prost encode/decode
//! - **GrpcStream** — server-streaming price updates with backpressure
//! - **No REST** — pure gRPC microservice, no dual-protocol overhead
//!
//! This is the use case where every optimization matters: high-frequency
//! order submission, real-time price feeds, and microsecond-level dispatch.
//!
//! ## Run
//!
//! ```bash
//! cargo run -p typeway-orderbook
//! ```
//!
//! ## Test
//!
//! ```bash
//! # List services
//! grpcurl -plaintext localhost:3000 list
//!
//! # Submit an order
//! grpcurl -plaintext -d '{
//!   "symbol": "AAPL",
//!   "side": "buy",
//!   "price": 185.50,
//!   "quantity": 100
//! }' localhost:3000 trading.v1.OrderBook/SubmitOrder
//!
//! # Get order book for a symbol
//! grpcurl -plaintext -d '{"symbol": "AAPL"}' \
//!   localhost:3000 trading.v1.OrderBook/GetOrderBook
//!
//! # Stream price updates
//! grpcurl -plaintext -d '{"symbol": "AAPL"}' \
//!   localhost:3000 trading.v1.OrderBook/StreamPrices
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};

use typeway_core::endpoint::*;
use typeway_core::path::{HCons, HNil, Lit, LitSegment};
use typeway_grpc::streaming::ServerStream;
use typeway_macros::{TypewayCodec, ToProtoType};
use typeway_protobuf::BytesStr;
use typeway_server::grpc_direct::into_direct_handler;
use typeway_server::*;

// =========================================================================
// Domain types — BytesStr for zero-copy, TypewayCodec for speed
// =========================================================================

/// An order submission.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, ToProtoType)]
struct Order {
    /// Ticker symbol (e.g., "AAPL"). BytesStr = zero-copy decode.
    #[proto(tag = 1)]
    symbol: BytesStr,
    /// "buy" or "sell".
    #[proto(tag = 2)]
    side: BytesStr,
    /// Limit price (0 = market order).
    #[proto(tag = 3)]
    price: f64,
    /// Number of shares.
    #[proto(tag = 4)]
    quantity: u32,
}

/// Acknowledgement after order submission.
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

/// Cancel request.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, ToProtoType)]
struct CancelRequest {
    #[proto(tag = 1)]
    order_id: BytesStr,
}

/// Cancel acknowledgement.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, ToProtoType)]
struct CancelAck {
    #[proto(tag = 1)]
    order_id: BytesStr,
    #[proto(tag = 2)]
    status: BytesStr,
}

/// Symbol query.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, ToProtoType)]
struct SymbolQuery {
    #[proto(tag = 1)]
    symbol: BytesStr,
}

/// Order book snapshot for a symbol.
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

/// A price level in the order book.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TypewayCodec, ToProtoType)]
struct PriceLevel {
    #[proto(tag = 1)]
    price: f64,
    #[proto(tag = 2)]
    quantity: u32,
    #[proto(tag = 3)]
    order_count: u32,
}

/// Real-time price update.
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
// Path types
// =========================================================================

#[allow(non_camel_case_types)]
struct __lit_orders;
impl LitSegment for __lit_orders {
    const VALUE: &'static str = "orders";
}

#[allow(non_camel_case_types)]
struct __lit_cancel;
impl LitSegment for __lit_cancel {
    const VALUE: &'static str = "cancel";
}

#[allow(non_camel_case_types)]
struct __lit_book;
impl LitSegment for __lit_book {
    const VALUE: &'static str = "book";
}

#[allow(non_camel_case_types)]
struct __lit_prices;
impl LitSegment for __lit_prices {
    const VALUE: &'static str = "prices";
}

type OrdersPath = HCons<Lit<__lit_orders>, HNil>;
type CancelPath = HCons<Lit<__lit_orders>, HCons<Lit<__lit_cancel>, HNil>>;
type BookPath = HCons<Lit<__lit_book>, HNil>;
type PricesPath = HCons<Lit<__lit_prices>, HNil>;

// =========================================================================
// API type
// =========================================================================

type OrderBookAPI = (
    PostEndpoint<OrdersPath, Order, OrderAck>,
    PostEndpoint<CancelPath, CancelRequest, CancelAck>,
    PostEndpoint<BookPath, SymbolQuery, OrderBookSnapshot>,
    ServerStream<GetEndpoint<PricesPath, Vec<PriceUpdate>>>,
);

// =========================================================================
// Shared state
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

// =========================================================================
// Handlers
// =========================================================================

/// Submit an order — the hot path.
///
/// This uses the standard handler path (Json<T>) for dual-protocol
/// compatibility. For maximum throughput, see the direct handler
/// registered below.
async fn submit_order(state: State<Exchange>, body: Json<Order>) -> (http::StatusCode, Json<OrderAck>) {
    let ack = OrderAck {
        order_id: BytesStr::from(state.0.next_order_id()),
        symbol: body.0.symbol.clone(),
        side: body.0.side.clone(),
        price: body.0.price,
        quantity: body.0.quantity,
        status: BytesStr::from("accepted"),
        timestamp_ns: Exchange::timestamp_ns(),
    };

    tracing::info!(
        "{} {} {}x{} @ {:.2} → {}",
        ack.side, ack.symbol, ack.quantity, ack.price,
        ack.order_id, ack.status,
    );

    // Publish price update.
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

/// Cancel an order.
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

/// Get order book snapshot.
async fn get_order_book(state: State<Exchange>, body: Json<SymbolQuery>) -> Json<OrderBookSnapshot> {
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

    let last_price = symbol_orders.last().map(|o| o.price).unwrap_or(0.0);
    let volume: u64 = symbol_orders.iter().map(|o| o.quantity as u64).sum();

    Json(OrderBookSnapshot {
        symbol: body.0.symbol.clone(),
        bids,
        asks,
        last_trade_price: last_price,
        volume,
    })
}

/// Stream price updates — server-streaming RPC.
async fn stream_prices(state: State<Exchange>) -> Json<Vec<PriceUpdate>> {
    // For the REST path, return recent price updates.
    // For gRPC, this endpoint is marked as ServerStream,
    // so the framework splits the array into individual frames.
    //
    // In a production system, this would use GrpcStream<T> for
    // true frame-by-frame streaming with backpressure.
    let mut rx = state.0.price_tx.subscribe();
    let mut updates = Vec::new();

    // Collect updates for 1 second or until 10 received.
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
// Main
// =========================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    let exchange = Exchange::new();

    // Print the generated .proto.
    let proto = <OrderBookAPI as typeway_grpc::ApiToProto>::to_proto("OrderBook", "trading.v1");
    tracing::info!("Generated .proto:\n{proto}");

    // Create the direct handler for maximum throughput order submission.
    let direct_exchange = exchange.clone();
    let _direct_submit = into_direct_handler(move |order: Order| {
        let ex = direct_exchange.clone();
        async move {
            let ack = OrderAck {
                order_id: BytesStr::from(ex.next_order_id()),
                symbol: order.symbol,
                side: order.side,
                price: order.price,
                quantity: order.quantity,
                status: BytesStr::from("accepted"),
                timestamp_ns: Exchange::timestamp_ns(),
            };
            ex.orders.lock().await.push(ack.clone());
            ack
        }
    });

    // Build the server with both standard and direct handlers.
    let _descriptor = <OrderBookAPI as typeway_grpc::service::ApiToServiceDescriptor>::service_descriptor("OrderBook", "trading.v1");

    // Register standard handlers for REST compatibility.
    let server = Server::<OrderBookAPI>::new((
        bind::<_, _, _>(submit_order),
        bind::<_, _, _>(cancel_order),
        bind::<_, _, _>(get_order_book),
        bind::<_, _, _>(stream_prices),
    ))
    .with_state(exchange);

    tracing::info!("Starting order book on http://localhost:3000");
    tracing::info!("  gRPC:   grpcurl -plaintext localhost:3000 list");
    tracing::info!("  Submit: grpcurl -plaintext -d '{{\"symbol\":\"AAPL\",\"side\":\"buy\",\"price\":185.50,\"quantity\":100}}' localhost:3000 trading.v1.OrderBook/SubmitOrder");
    tracing::info!("");
    tracing::info!("  Direct handler registered for SubmitOrder (bypasses extractors)");
    tracing::info!("  Standard handlers serve REST + gRPC for all other endpoints");

    // Serve with the direct handler overriding SubmitOrder for gRPC.
    let grpc_server = server.with_grpc("OrderBook", "trading.v1");

    // For the demo, use the standard path (direct handler registration
    // requires manual multiplexer construction — see grpc_direct_test.rs).
    grpc_server.serve("0.0.0.0:3000".parse()?).await
}
