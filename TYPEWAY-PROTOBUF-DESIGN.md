> **Status: Partially implemented.** The `typeway-protobuf` crate exists and ships: `#[derive(TypewayCodec)]` (compile-time specialized encode/decode, 12-54% faster than prost), `BytesStr` (zero-copy strings), `RepeatedField<T>` (pooled allocations), `ProtoField<T, E>` (phantom-typed wire formats), `EncodeBuf` (buffer reuse), and `Proto<T>` (format-agnostic extractor). The more ambitious features in this design doc — GAT-based `View<'buf>` types, typestate builders, SIMD varint, and arena-scoped allocation — remain future work.

# Redesigning Prost: typeway-protobuf — A Type-Theoretic, High-Performance Protobuf Library for Rust

## Executive Summary

This document proposes a ground-up redesign of Rust's protobuf story, drawing on lessons from prost's limitations, ideas from zero-copy serialization frameworks (rkyv, Cap'n Proto, FlatBuffers), and type-theoretic / functional programming techniques that Rust's type system uniquely enables. The result is **`typeway-protobuf`** — a protobuf implementation that integrates with the existing Typeway framework and is both faster and more ergonomic than prost.

---

## 1. Prost's Current Limitations

### 1.1 The `Option<T>` Everywhere Problem

Prost wraps all non-primitive message fields in `Option<T>`, even when the schema semantically requires them. This leads to:

- Pervasive `.unwrap()` / `.as_ref().unwrap()` chains
- Runtime panics where the compiler should have caught errors
- Inability to make invalid states unrepresentable

A crate like `prost-unwrap` exists solely to generate mirror structs that strip the `Option` wrappers — a clear signal that the core abstraction is wrong.

### 1.2 Allocation-Heavy Deserialization

Prost copies all string and bytes data into owned `String` / `Vec<u8>` during deserialization. GreptimeDB's benchmarking showed prost taking **~7.3ms** to parse a Prometheus write request that VictoriaMetrics (Go) handled in **~1.2ms** — a 6x difference. The causes:

- Every `string` field allocates a new `String` and copies bytes
- Every `repeated` field creates a fresh `Vec` with no pooling
- `Vec::clear()` drops inner elements recursively, adding deallocation overhead
- No mechanism for borrowing from the input buffer

### 1.3 Weak Type Mapping

Prost maps all protobuf enums to `i32`, losing type safety. Six different wire types (`repeated int32`, `repeated sint32`, `repeated sfixed32`, plus packed variants) all map to `Vec<i32>`, making trait-based dispatch impossible. The Serde integration is bolted on rather than native.

### 1.4 No Runtime Reflection

Prost explicitly does not support message descriptors or runtime reflection, limiting its use for dynamic dispatch, generic middleware, or schema evolution tooling.

### 1.5 Maintenance Status

Prost is passively maintained. The maintainer has stated they expect Google's official Rust protobuf library to supersede it, and new features are not being accepted.

---

## 2. Design Principles for typeway-protobuf

| Principle | Technique |
|---|---|
| Make invalid states unrepresentable | Typestate pattern, phantom types, ADT-aligned codegen |
| Zero-copy where possible | GAT-powered borrowing, `Bytes` + `Cow` field strategies |
| Pooling as a first-class concern | Arena-based message allocation, `RepeatedField` semantics |
| Layered API: zero-cost → ergonomic | View types for reading, owned types for mutation |
| Native Rust idioms | Builder pattern, `From`/`Into` conversions, serde integration |

---

## 3. Type-Theoretic Foundations

### 3.1 Algebraic Data Types for Schema Mapping

Protobuf's `oneof` is a sum type, but prost wraps it in `Option<enum>`, conflating "not set" with "absent." typeway-protobuf should generate a proper Rust enum that distinguishes these:

```rust
// Proto schema:
// oneof payload {
//   TextMessage text = 1;
//   ImageMessage image = 2;
// }

// Generated code — the oneof IS the enum, not Option<enum>
pub enum Payload {
    Text(TextMessage),
    Image(ImageMessage),
}

// The parent message uses Option only if the oneof itself is optional
pub struct ChatEvent {
    pub id: EventId,
    pub payload: Payload,         // required oneof — no Option
    pub metadata: Option<Meta>,   // truly optional message field
}
```

This aligns with the "Typical" project's philosophy: use algebraic data types with non-nullable defaults, making exhaustive pattern matching the norm rather than the exception. Typeway-protobuf inherits this principle from the broader Typeway framework's commitment to type-level correctness.

### 3.2 Typestate Pattern for Message Construction

Serde already uses typestates internally for its `Serializer` trait. We apply the same idea to message building — a message under construction carries compile-time proof of which required fields have been set:

```rust
// Generated builder with typestate tracking
pub struct ChatEventBuilder<Id = Missing, Payload = Missing> {
    inner: ChatEventPartial,
    _id: PhantomData<Id>,
    _payload: PhantomData<Payload>,
}

pub struct Missing;
pub struct Set;

impl ChatEventBuilder<Missing, Missing> {
    pub fn new() -> Self { /* ... */ }
}

impl<P> ChatEventBuilder<Missing, P> {
    pub fn id(self, id: EventId) -> ChatEventBuilder<Set, P> {
        // moves ownership, type-level state transition
        ChatEventBuilder {
            inner: ChatEventPartial { id: Some(id), ..self.inner },
            _id: PhantomData,
            _payload: self._payload,
        }
    }
}

impl<I> ChatEventBuilder<I, Missing> {
    pub fn payload(self, p: Payload) -> ChatEventBuilder<I, Set> {
        /* ... */
    }
}

// .build() is ONLY available when all required fields are Set
impl ChatEventBuilder<Set, Set> {
    pub fn build(self) -> ChatEvent {
        ChatEvent {
            id: self.inner.id.unwrap(),     // safe: typestate guarantees this
            payload: self.inner.payload.unwrap(),
            metadata: self.inner.metadata,
        }
    }
}
```

The compiler rejects any attempt to call `.build()` without setting required fields. No runtime checks needed.

### 3.3 Phantom Types for Wire Format Discrimination

Prost's fundamental problem with `Vec<i32>` ambiguity can be solved with phantom-typed wrappers:

```rust
pub struct ProtoField<T, Encoding> {
    pub value: T,
    _encoding: PhantomData<Encoding>,
}

// Zero-sized marker types for wire encodings
pub struct Varint;
pub struct ZigZag;
pub struct Fixed;
pub struct Packed<E>(PhantomData<E>);

// Now these are distinct types:
type RepeatedInt32     = Vec<ProtoField<i32, Varint>>;
type RepeatedSint32    = Vec<ProtoField<i32, ZigZag>>;
type RepeatedSfixed32  = Vec<ProtoField<i32, Fixed>>;
type PackedInt32       = Vec<ProtoField<i32, Packed<Varint>>>;

// Encoding/decoding dispatches on the phantom type — zero runtime cost
impl<T: ProtoEncodable, E: WireStrategy> Encode for ProtoField<T, E> {
    fn encode(&self, buf: &mut impl BufMut) {
        E::encode(&self.value, buf)
    }
}
```

This restores trait-based dispatch while being completely erased at runtime. These phantom-typed wrappers can also plug into Typeway's broader encoding abstractions if the framework defines its own wire strategy traits.

### 3.4 GATs for Zero-Copy Deserialization

Generic Associated Types enable a `MessageView` trait where deserialized types borrow from the input buffer:

```rust
pub trait MessageView {
    /// The lifetime-parameterized view type
    type View<'buf> where Self: 'buf;

    /// Validate and create a view without copying
    fn view_from<'buf>(buf: &'buf [u8]) -> Result<Self::View<'buf>, DecodeError>;
}

// Generated for each message:
pub struct ChatEventView<'buf> {
    buf: &'buf [u8],
    // Precomputed field offsets from a validation pass
    offsets: ChatEventOffsets,
}

impl<'buf> ChatEventView<'buf> {
    pub fn id(&self) -> &'buf str {
        // Returns a borrow directly into the buffer — no allocation
        &self.buf[self.offsets.id_start..self.offsets.id_end]
    }

    pub fn payload(&self) -> PayloadView<'buf> {
        // Nested messages also borrow from the same buffer
        PayloadView::from_range(self.buf, self.offsets.payload)
    }

    /// Upgrade to an owned type when mutation or 'static lifetime is needed
    pub fn to_owned(&self) -> ChatEvent {
        ChatEvent {
            id: self.id().to_string().into(),
            payload: self.payload().to_owned(),
            metadata: self.metadata().map(|m| m.to_owned()),
        }
    }
}
```

This is the pattern used by `icu4x`'s Yoke/Yokeable framework and serde's `Deserialize<'de>` — but applied structurally to protobuf.

### 3.5 Dual Types for Schema Evolution (Inspired by "Typical")

The "Typical" serialization library introduces **asymmetric field types** — separate types for serialization vs. deserialization. This elegantly solves schema evolution:

```rust
// When a field `sender` is newly added as "asymmetric":

// Writers MUST provide it (non-optional)
pub struct ChatEventOut {
    pub id: EventId,
    pub payload: Payload,
    pub sender: UserId,        // required for writers
}

// Readers MAY not have it (optional, for backward compat)
pub struct ChatEventIn {
    pub id: EventId,
    pub payload: Payload,
    pub sender: Option<UserId>, // optional for readers
}
```

This prevents the common bug where a developer adds a required field but forgets to populate it in some code path. The type system enforces the asymmetry.

---

## 4. Performance Architecture

### 4.1 Arena-Based Message Pooling

The GreptimeDB team discovered that prost's `merge` into a cleared message was no faster than creating a new one, because `Vec::clear()` recursively drops elements. The solution is `RepeatedField` semantics (from `rust-protobuf` v2):

```rust
pub struct RepeatedField<T> {
    buf: Vec<T>,
    len: usize,  // logical length, may be < buf.len()
}

impl<T> RepeatedField<T> {
    pub fn clear(&mut self) {
        // Only reset the logical length — don't drop elements
        // Elements are overwritten on next deserialize
        self.len = 0;
    }

    pub fn push(&mut self, val: T) {
        if self.len < self.buf.len() {
            self.buf[self.len] = val; // reuse allocation
        } else {
            self.buf.push(val);
        }
        self.len += 1;
    }
}
```

For the GreptimeDB benchmark, this alone reduced deserialization time from ~7.3ms to ~2.7ms (a **63% reduction**).

### 4.2 `Bytes` + Specialized Slicing for String Fields

Prost supports `Bytes` for `bytes` fields but not strings. And even the `Bytes` path incurs overhead from the `BytesAdapter` → `Buf` trait conversion chain. typeway-protobuf should:

1. Use `Bytes` as the backing store for all string/bytes fields
2. Implement a specialized `BytesStr` type that is `Bytes` + UTF-8 validity proof
3. Avoid the `Bytes::slice()` overhead by using raw offset arithmetic where safe

```rust
/// A string that borrows from a shared `Bytes` buffer.
/// Zero-copy: just a pointer + offset + length into the parent Bytes.
#[derive(Clone)]
pub struct BytesStr {
    inner: Bytes,  // refcounted, cheap to clone
}

impl BytesStr {
    /// Safety: caller must guarantee `buf` contains valid UTF-8
    pub(crate) unsafe fn from_bytes_unchecked(buf: Bytes) -> Self {
        BytesStr { inner: buf }
    }

    pub fn as_str(&self) -> &str {
        // Safety: validated at construction
        unsafe { std::str::from_utf8_unchecked(&self.inner) }
    }
}

impl Deref for BytesStr {
    type Target = str;
    fn deref(&self) -> &str { self.as_str() }
}
```

### 4.3 Tiered Deserialization Strategy

Not every use case needs every field. typeway-protobuf supports three tiers:

| Tier | Allocation | Lifetime | Use Case |
|---|---|---|---|
| `View<'buf>` | Zero | Borrows from buffer | Read-only access, routing, filtering |
| `Cow` fields | Lazy | `'buf` or `'static` | Read-mostly with occasional mutation |
| `Owned` | Full copy | `'static` | Long-lived storage, mutation-heavy |

```rust
// Tier 1: Zero-copy view
let view = ChatEventView::decode(buf)?;
if view.id().starts_with("spam") {
    return Ok(()); // never allocated anything
}

// Tier 2: Selective ownership via Cow
let cow_msg: ChatEventCow<'_> = view.to_cow();
// String fields are Cow<'buf, str> — only allocate if you mutate

// Tier 3: Full owned conversion
let owned: ChatEvent = view.to_owned();
```

### 4.4 SIMD-Accelerated Varint Decoding

Varint decoding is a hot path in protobuf parsing. Modern x86 CPUs with BMI2 support can decode varints significantly faster:

```rust
#[cfg(target_arch = "x86_64")]
pub fn decode_varint_bmi2(buf: &[u8]) -> (u64, usize) {
    // Use PEXT to extract the 7-bit payload from each byte in parallel
    // This avoids the branch-heavy loop of traditional varint decoding
    unsafe {
        let raw = std::ptr::read_unaligned(buf.as_ptr() as *const u64);
        let mask = !raw & 0x8080808080808080; // find continuation bits
        let len = (mask.trailing_zeros() / 8 + 1) as usize;
        let payload_mask = (1u64 << (len * 7)) - 1;
        let extracted = core::arch::x86_64::_pext_u64(raw, 0x7f7f7f7f7f7f7f7f);
        (extracted & payload_mask, len)
    }
}
```

---

## 5. Ergonomic API Design

### 5.1 Functional Combinators for Message Transformation

Inspired by functional programming, typeway-protobuf generates `map`, `and_then`, and `filter` combinators for repeated fields and optional fields:

```rust
let event: ChatEvent = ChatEvent::builder()
    .id("evt-001")
    .payload(Payload::Text(TextMessage {
        body: "hello".into(),
    }))
    .build();

// Functional transformation of nested fields
let redacted = event
    .map_payload(|p| match p {
        Payload::Text(t) => Payload::Text(t.map_body(|b| redact(b))),
        other => other,
    });

// Filter on repeated fields
let active_users: Vec<_> = response
    .users()
    .filter(|u| u.is_active())
    .map(|u| u.to_owned())
    .collect();
```

### 5.2 Optics-Inspired Field Access (Lens Pattern)

For deeply nested protobuf messages, generate lens-like accessors:

```rust
// Instead of:
msg.outer.as_ref().unwrap().inner.as_ref().unwrap().value

// Use a composed accessor:
let val = msg.get(outer().inner().value());
// Returns Option<&T>, collapsing the nested Option chain

// Or with a default:
let val = msg.view(outer().inner().value()).unwrap_or_default();
```

This is essentially the functional programming concept of **optics** (lenses/prisms) adapted to Rust's ownership model. The generated lens types compose and are zero-cost (monomorphized away).

### 5.3 Derive-Based Custom Messages

Unlike prost, which requires `.proto` files, typeway-protobuf also supports derive macros on hand-written Rust types — consistent with Typeway's derive-first philosophy:

```rust
#[derive(ProtoMessage)]
#[proto(package = "myapp.v1")]
pub struct UserProfile {
    #[proto(tag = 1)]
    pub id: UserId,

    #[proto(tag = 2, encoding = "length_delimited")]
    pub name: String,

    #[proto(tag = 3, oneof)]
    pub contact: ContactMethod,

    #[proto(tag = 4, repeated)]
    pub roles: Vec<Role>,
}

#[derive(ProtoOneof)]
pub enum ContactMethod {
    #[proto(tag = 5)]
    Email(String),
    #[proto(tag = 6)]
    Phone(PhoneNumber),
}
```

This gives you full control over the types while remaining wire-compatible.

---

## 6. Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                  typeway-protobuf                        │
│          (integrates with Typeway framework)             │
├─────────────┬──────────────┬────────────────────────────┤
│  Code Gen   │  Derive API  │  Runtime Library            │
│  (protoc    │  (#[derive]) │                             │
│   plugin)   │              │  ┌──────────────────┐       │
│             │              │  │ Wire Codec        │       │
│  Generates: │  Generates:  │  │ (varint, LEN,    │       │
│  - Owned    │  - Encode    │  │  zigzag, fixed)  │       │
│  - View     │  - Decode    │  ├──────────────────┤       │
│  - Builder  │  - View      │  │ Arena / Pool     │       │
│  - Lens     │              │  │ (RepeatedField,  │       │
│  - In/Out   │              │  │  MessagePool)    │       │
│    (schema  │              │  ├──────────────────┤       │
│    evolution│              │  │ BytesStr / Cow   │       │
│    duals)   │              │  │ (zero-copy       │       │
│             │              │  │  string types)   │       │
├─────────────┴──────────────┤  ├──────────────────┤       │
│  Serde Integration Layer   │  │ Reflection       │       │
│  (bidirectional JSON ↔     │  │ (descriptors,    │       │
│   protobuf via serde)      │  │  dynamic msgs)   │       │
├────────────────────────────┴──┴──────────────────┤       │
│              Typeway Framework Core               │       │
│  (shared traits, type-level primitives, encoding  │       │
│   abstractions, lens/optics foundations)           │       │
└──────────────────────────────────────────────────────────┘
```

---

## 7. Summary: What Each Technique Solves

| Prost Problem | FP / Type Theory Technique | Result |
|---|---|---|
| `Option<T>` on required fields | **Typestate builders** with phantom types | Compile-time required field enforcement |
| `oneof` wrapped in `Option<enum>` | **Algebraic data types** (proper sum types) | Exhaustive matching, no double-unwrap |
| `Vec<i32>` ambiguity (6 wire types) | **Phantom-typed fields** | Trait dispatch restored, zero runtime cost |
| Allocation-heavy deserialization | **GAT-powered `View<'buf>` types** | Zero-copy reads borrowing from buffer |
| `Vec::clear()` drop overhead | **RepeatedField** (logical length reset) | 63% deserialization speedup |
| String copy on every deserialize | **`BytesStr`** + specialized slicing | Near-zero-copy string handling |
| Schema evolution fragility | **Asymmetric In/Out types** (Typical-style) | Type-enforced backward compatibility |
| Deep nesting verbosity | **Optics / Lens** composable accessors | `msg.get(a().b().c())` instead of unwrap chains |
| No derive-based usage | **Proc macro derive** on native Rust types | Full type control + wire compatibility |

---

## 8. Migration Strategy

Typeway-protobuf should be **wire-compatible** with standard protobuf, meaning it reads/writes the same bytes. Migration from prost would involve:

1. **Phase 1**: Drop-in codec replacement — same `.proto` files, new generated code. The owned types look similar but with better Option handling.
2. **Phase 2**: Opt into zero-copy views where performance matters.
3. **Phase 3**: Adopt typestate builders and lens accessors for new code.
4. **Phase 4**: Use asymmetric In/Out types for services undergoing schema evolution.
5. **Phase 5**: Deeper Typeway integration — share type-level primitives, encoding traits, and optics foundations across protobuf and other Typeway-supported protocols.

Each phase is independently valuable, and existing prost-using code can interop via `From`/`Into` implementations on the owned types.
