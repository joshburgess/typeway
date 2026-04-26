# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0

### added:

- **`#[derive(TypewayCodec)]`**: compile-time specialized protobuf encode/decode. 12-40% faster encode and 24-54% faster decode vs. `#[derive(prost::Message)]` (Criterion benchmarks; see `BENCHMARKS.md`). Supports structs, simple enums (varint), and tagged enums (oneof).
- **`#[derive(TypestateBuilder)]`**: compile-time enforced message construction with `#[required]` field markers
- **`BytesStr`**: zero-copy string type backed by `bytes::Bytes`. `Deref<Target = str>` skips redundant UTF-8 re-validation at the type boundary
- **`RepeatedField<T>`**: pooled allocations for repeated fields
- **`ProtoField<T, E>`**: phantom-typed wire formats (e.g., `Sint32`, `Fixed64`) without changing the runtime representation
- **`EncodeBuf`**: reusable encode buffer with `encode(msg)` API for steady-state allocation elimination
- **`BufPool`**: thread-safe arena-style buffer pool. Pre-allocate N buffers of M bytes and borrow from the pool
- **`MessageView<'buf>`**: GAT-based zero-copy borrowed decode with `ViewStr<'buf>` and `ViewBytes<'buf>`. Fields are slices into the input buffer, no allocation
- **`tw_decode_packed_varints`**: batch varint decode with inline 1-byte and 2-byte fast paths
- **`Proto<T>`**: format-agnostic extractor for handlers (works with both REST JSON and gRPC binary)
- **Performance tricks** (documented in `BENCHMARKS.md`):
  - O(1) varint length via bit-shift formula
  - Pre-validated UTF-8 + `String::from_utf8_unchecked`
  - Single-pass packed encode (write data, backfill length)
  - Bulk memcpy for packed fixed types on little-endian
  - Inline tag bytes for fields 1-15
- **`UNSAFE.md`**: catalog of every `unsafe` block, the invariants it relies on, and why it is sound. The user-facing API surface is entirely safe (`#[doc(hidden)]` on internal helpers)
