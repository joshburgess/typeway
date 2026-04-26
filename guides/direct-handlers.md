# Direct gRPC Handlers for Maximum Throughput

typeway's default handler path (`Json<T>` or `Proto<T>`) shares handlers
between REST and gRPC. This adds ~1-2 µs of dispatch overhead per request
(content-type detection, extractor pipeline, HTTP parts construction).

For gRPC-only microservices where every microsecond matters, **direct
handlers** bypass this pipeline entirely.

## The tradeoff

| Handler style | Latency | Serves REST? | Serves gRPC? |
|---------------|---------|-------------|-------------|
| `Json<T>` | baseline | Yes | Yes |
| `Proto<T>` | -4% | Yes | Yes |
| **Direct** | **-6%** | **No** | **Yes** |

Direct handlers give you the fastest possible gRPC dispatch, within
measurement noise of Tonic.

## How to use

```rust
use typeway::prelude::*;

#[derive(TypewayCodec, Serialize, Deserialize, Default)]
struct CreateOrder {
    #[proto(tag = 1)]
    item: String,
    #[proto(tag = 2)]
    quantity: u32,
}

#[derive(TypewayCodec, Serialize, Deserialize, Default)]
struct Order {
    #[proto(tag = 1)]
    id: u64,
    #[proto(tag = 2)]
    item: String,
    #[proto(tag = 3)]
    quantity: u32,
    #[proto(tag = 4)]
    status: String,
}

// Define the handler as a plain async function.
// No extractors, no Proto<T>, no Json<T>, just types in, types out.
async fn create_order(req: CreateOrder) -> Order {
    Order {
        id: 42,
        item: req.item,
        quantity: req.quantity,
        status: "pending".into(),
    }
}

// Register it as a direct handler
let handler = into_direct_handler(create_order);
```

The direct handler:
- Decodes the request via `TypewayDecode` (binary protobuf)
- Calls your function directly (no trait object, no extractors)
- Encodes the response via `TypewayEncode`
- Returns a gRPC-framed response with proper trailers

## When NOT to use direct handlers

- Your handler also needs to serve REST clients
- Your handler uses extractors (`Path<T>`, `State<T>`, `Query<T>`)
- You need middleware that operates on HTTP parts (auth, logging)
- The latency difference doesn't matter for your workload

For most applications, `Proto<T>` is the right choice. Direct handlers
are for hot paths in gRPC-only services where you've profiled and
confirmed that dispatch overhead is measurable.

## How it works

Normal path (Proto<T>):
```
gRPC binary → collect body → build HTTP Parts → content-type check →
Proto<T>::from_request (TypewayDecode) → handler → Proto<T>::into_response
(serde JSON) → wrap in gRPC frame → send
```

Direct path:
```
gRPC binary → collect body → strip frame → TypewayDecode → handler →
TypewayEncode → frame → send
```

The direct path skips HTTP Parts construction, content-type detection,
the extractor trait dispatch, and response body boxing.
