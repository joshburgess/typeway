# Optimizing Decode Performance with BytesStr

By default, protobuf string fields decode into `String`, which allocates
heap memory for every field on every request. For high-throughput gRPC
services, these allocations add up.

`BytesStr` eliminates them by slicing the input buffer instead of copying.

## When to use BytesStr

**Use `String`** (default) when:
- You're not bottlenecked on decode performance
- You need to mutate the string after decoding
- You want maximum simplicity

**Use `BytesStr`** when:
- You handle thousands of requests per second
- Your messages have many string fields
- You need to read string fields without modifying them
- Profiling shows protobuf decode as a hotspot

## Automatic with proto-first codegen

If you generate types from a `.proto` file using `proto_to_typeway_with_codec()`,
**BytesStr is used automatically** for all `string` fields — no manual work needed.
See the [proto-first codegen guide](proto-first-codegen.md).

## Manual switch (Rust-first)

If you define types by hand, replace `String` with `BytesStr` on the fields
you want to optimize:

```rust
use typeway_protobuf::BytesStr;

// Before: allocates per field
#[derive(TypewayCodec, Serialize, Deserialize, Default)]
struct UserProfile {
    #[proto(tag = 1)]
    id: u64,
    #[proto(tag = 2)]
    username: String,
    #[proto(tag = 3)]
    email: String,
    #[proto(tag = 4)]
    bio: String,
}

// After: zero-copy decode for string fields
#[derive(TypewayCodec, Serialize, Deserialize, Default)]
struct UserProfile {
    #[proto(tag = 1)]
    id: u64,
    #[proto(tag = 2)]
    username: BytesStr,
    #[proto(tag = 3)]
    email: BytesStr,
    #[proto(tag = 4)]
    bio: BytesStr,
}
```

## What changes

`BytesStr` implements `Deref<Target = str>`, so most code works unchanged:

```rust
// These all work with both String and BytesStr:
println!("{}", profile.username);
if profile.email.contains("@example.com") { /* ... */ }
let greeting = format!("Hello, {}!", profile.username);
```

What doesn't work: `push_str`, `insert`, or any mutation. `BytesStr` is
immutable — it's a view into the original buffer. If you need to mutate,
call `.to_string()` to get an owned `String`.

## Performance impact

Benchmarked with Criterion (medium message, 7 fields, 4 strings):

| Decode | Time | vs prost |
|--------|------|----------|
| `String` fields | 63 ns | 37% faster |
| `BytesStr` fields | 46 ns | 54% faster |
| prost | 100 ns | — |

The improvement comes from eliminating 4 heap allocations. Each `String`
field does `malloc + memcpy`. `BytesStr` does a refcount increment on the
shared `Bytes` buffer — effectively free.

## How it works

When you use `BytesStr` fields and decode via `typeway_decode_bytes(Bytes)`:

1. The decoder validates UTF-8 on the borrowed slice (zero-copy)
2. It calls `Bytes::slice(offset..end)` — increments a refcount
3. The `BytesStr` wraps the slice — no allocation, no copy

When you use `String` fields:

1. The decoder validates UTF-8 on the borrowed slice
2. It calls `slice.to_vec()` — allocates + copies
3. The `String` wraps the owned `Vec<u8>`

## Mixing String and BytesStr

You can mix them in the same struct. Use `BytesStr` for fields you read
but don't modify, and `String` for fields you need to mutate:

```rust
#[derive(TypewayCodec, Serialize, Deserialize, Default)]
struct Article {
    #[proto(tag = 1)]
    id: u64,
    #[proto(tag = 2)]
    title: BytesStr,      // read-only, zero-copy
    #[proto(tag = 3)]
    slug: BytesStr,       // read-only, zero-copy
    #[proto(tag = 4)]
    body: String,         // might need mutation (editing)
    #[proto(tag = 5)]
    author: BytesStr,     // read-only, zero-copy
}
```

## Serialization

`BytesStr` implements `serde::Serialize` and `serde::Deserialize`, so it
works transparently with JSON, TOML, or any serde format. When serialized
to JSON, it produces a regular string. When deserialized from JSON, it
allocates (same as `String`) — the zero-copy advantage is protobuf-specific.
