# typeway-protobuf Benchmarks

All benchmarks run with Criterion on the same machine, same Rust toolchain.
Message schemas are identical between typeway-protobuf and prost.

## Codec: typeway-protobuf vs prost

typeway-protobuf uses `#[derive(TypewayCodec)]` (compile-time specialized).
prost uses `#[derive(prost::Message)]`.

### Encode (Rust struct → protobuf binary)

| Message | typeway-protobuf | prost | Advantage |
|---------|-----------------|-------|-----------|
| Small (2 fields, 1 string) | 14.0 ns | 15.9 ns | **12% faster** |
| Medium (7 fields, 4 strings) | 23.7 ns | 30.1 ns | **21% faster** |
| Large (9 fields, 6 strings) | 65.6 ns | 81.7 ns | **20% faster** |
| Packed (100 u32 + 20 f64) | 102 ns | 171 ns | **40% faster** |

### Decode (protobuf binary → Rust struct)

| Message | typeway-protobuf | prost | Advantage |
|---------|-----------------|-------|-----------|
| Small (String fields) | 18.8 ns | 31.4 ns | **40% faster** |
| Small (BytesStr zero-copy) | 16.2 ns | 31.4 ns | **48% faster** |
| Medium (String fields) | 62.6 ns | 100 ns | **37% faster** |
| Medium (BytesStr zero-copy) | 45.6 ns | 100 ns | **54% faster** |
| Large (String fields) | 273 ns | 358 ns | **24% faster** |
| Large (BytesStr zero-copy) | 202 ns | 358 ns | **44% faster** |
| Packed (100 u32 + 20 f64) | 195 ns | 416 ns | **53% faster** |

### Why typeway-protobuf is faster

1. **Compile-time field layout**, tag numbers, wire types, and tag bytes
   are constants in the generated code. No runtime dispatch.
2. **O(1) varint length**, `(ilog2 * 9 + 73) / 64` (bit shift) instead of
   `div_ceil(7)` (integer division, 10x slower).
3. **Pre-validated UTF-8**, validate on borrowed slice, then
   `String::from_utf8_unchecked` skips redundant re-validation.
4. **BytesStr zero-copy**, `Bytes::slice()` instead of allocation for
   string fields. Eliminates all string allocations on decode.
5. **Single-pass packed encode**, write data first, backfill length.
   Avoids double iteration.
6. **Batch unsafe varint write**, one `set_len` for entire packed field
   instead of per-element.
7. **Bulk memcpy for packed fixed types**, `Vec<f64>` written as single
   `extend_from_slice` on little-endian (instead of per-element writes).
8. **Inline tag bytes**, `buf.push(0x08)` instead of function call for
   fields 1-15.

## End-to-end: typeway-grpc vs Tonic

Same CreateUser RPC (unary, small message). All servers use hyper HTTP/2.

| Server | Latency | vs Tonic |
|--------|---------|----------|
| Baseline (bare hyper, no framework) | 48.8 µs | N/A |
| Tonic (prost + async_trait) | 49.5 µs | N/A |
| typeway Direct (TypewayCodec, no extractors) | 49.9 µs | 0.8% slower |
| typeway Proto\<T\> (dual REST/gRPC) | 50.5 µs | 2.0% slower |
| typeway Json\<T\> (JSON codec) | 52.8 µs | 6.7% slower |

### Why the e2e gap exists

The codec is faster (12-54%), but the dispatch overhead absorbs the gain:

- **Proto\<T\> path** (+2%): synthetic HTTP Parts construction + extractor
  pipeline + content-type detection. This is the cost of dual REST/gRPC
  handler reuse.
- **Direct path** (+0.8%): `Arc<dyn Fn>` dispatch + `BoxBody` wrapping +
  multiplexer routing checks. Structural overhead of the framework.
- **The ~49 µs HTTP/2 round-trip dominates.** The 0.5-1.5 µs dispatch
  overhead is 1-3% of total. For real handlers with I/O (1-50 ms), the
  gap is unmeasurable.

### Handler tradeoffs

| Style | Latency | REST + gRPC? | When to use |
|-------|---------|-------------|-------------|
| `Json<T>` | +6.7% | Yes | Default, maximum compatibility |
| `Proto<T>` | +2.0% | Yes | Dual-protocol with good performance |
| Direct | +0.8% | No (gRPC only) | Maximum throughput, gRPC-only services |

## Additional optimizations

Beyond the core codec benchmarks above, typeway-protobuf provides:

- **`BufPool`**: thread-safe pool of reusable encode buffers. Eliminates
  per-request allocation in steady state. Pre-allocate N buffers of M bytes
  and borrow from the pool.
- **`MessageView<'buf>`**: GAT-based zero-copy borrowed decode. Fields are
  `&'buf str` slices into the input buffer, no allocation at all. For
  read-heavy workloads where you inspect a few fields and discard the rest.
- **`tw_decode_packed_varints`**: batch varint decode with inline 1-byte
  and 2-byte fast paths for packed repeated fields.
- **Enum support**: simple enums (varint) and tagged enums (oneof) with
  per-variant wire type dispatch, all compile-time specialized.
