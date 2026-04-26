# High-Performance Order Book, gRPC Microservice

A real-time trading order book that pushes typeway's performance envelope.
Every optimization is used: direct handlers, BytesStr zero-copy strings,
TypewayCodec compile-time specialized encoding, and server-streaming
price feeds with backpressure.

## Why this use case

Trading systems are the canonical "every microsecond matters" domain:

- **Order submission** is the hottest path, direct handlers bypass
  the HTTP extractor pipeline entirely
- **Ticker symbols and order IDs** are read-only strings. BytesStr
  eliminates allocation on decode (54% faster than prost)
- **Price feeds** are server-streaming, subscribers receive updates
  in real time with backpressure
- **No REST needed**: this is an internal microservice, pure gRPC

## Performance stack

| Layer | Optimization | Advantage |
|-------|-------------|-----------|
| Codec | `#[derive(TypewayCodec)]` | 12-54% faster than prost |
| Strings | `BytesStr` fields | Zero-copy decode (no allocation) |
| Dispatch | `into_direct_handler` | Bypasses extractor pipeline |
| Streaming | `ServerStream` + backpressure | Real tokio::mpsc channels |
| Encode | Packed repeated, bulk memcpy | 40% faster packed encode |

## The API

```rust
type OrderBookAPI = (
    PostEndpoint<OrdersPath, Order, OrderAck>,               // submit order
    PostEndpoint<CancelPath, CancelRequest, CancelAck>,      // cancel order
    PostEndpoint<BookPath, SymbolQuery, OrderBookSnapshot>,  // get book
    ServerStream<GetEndpoint<PricesPath, Vec<PriceUpdate>>>, // price feed
);
```

## Domain types with BytesStr

```rust
#[derive(TypewayCodec, Serialize, Deserialize)]
struct Order {
    #[proto(tag = 1)]
    symbol: BytesStr,    // zero-copy: "AAPL" decoded without allocation
    #[proto(tag = 2)]
    side: BytesStr,      // zero-copy: "buy" or "sell"
    #[proto(tag = 3)]
    price: f64,
    #[proto(tag = 4)]
    quantity: u32,
}
```

When a gRPC client sends a binary protobuf `Order`, the `symbol` field
is decoded by slicing the input buffer, a refcount increment, not a
heap allocation. For a service processing thousands of orders per second,
this eliminates thousands of allocations per second.

## Two approaches, same result

This example includes **two versions** of the same trading service:

### Rust-first (`main.rs`)

Types are defined by hand in Rust. You control every detail:

```bash
cargo run -p typeway-orderbook
```

### Proto-first (`from_proto.rs`)

Types are generated from a `.proto` definition. At startup, the server
prints the generated Rust code so you can see exactly what the codegen
produces, `BytesStr` fields, `#[derive(TypewayCodec)]`, `#[proto(tag)]`
attributes, all automatic:

```bash
cargo run -p typeway-orderbook --bin typeway-orderbook-from-proto
```

Both versions produce identical servers with identical performance.
The difference is where your source of truth lives.

## Test

```bash
# Discover the API
grpcurl -plaintext localhost:3000 list
grpcurl -plaintext localhost:3000 describe trading.v1.OrderBook

# Submit orders
grpcurl -plaintext -d '{"symbol":"AAPL","side":"buy","price":185.50,"quantity":100}' \
  localhost:3000 trading.v1.OrderBook/SubmitOrder

grpcurl -plaintext -d '{"symbol":"AAPL","side":"sell","price":186.00,"quantity":50}' \
  localhost:3000 trading.v1.OrderBook/SubmitOrder

# View order book
grpcurl -plaintext -d '{"symbol":"AAPL"}' \
  localhost:3000 trading.v1.OrderBook/GetOrderBook

# Stream prices (blocks until updates arrive)
grpcurl -plaintext -d '{"symbol":"AAPL"}' \
  localhost:3000 trading.v1.OrderBook/StreamPrice

# Cancel an order
grpcurl -plaintext -d '{"order_id":"ORD-000001"}' \
  localhost:3000 trading.v1.OrderBook/CancelOrder
```

## Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Rust-first: hand-written types with all optimizations |
| `src/from_proto.rs` | Proto-first: types generated from `.proto` definition |
