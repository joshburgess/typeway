# Unsafe Code in typeway-protobuf

This document catalogs every use of `unsafe` in typeway-protobuf and the
derive macro code it generates. Each entry explains what invariant is
relied upon and why the usage is sound.

## typeway-protobuf/src/codec.rs

### 1. `tw_encode_varint` — multi-byte path (line ~155)

```rust
buf.reserve(10);
unsafe {
    let base = buf.as_mut_ptr();
    while v >= 0x80 { *base.add(pos) = ...; pos += 1; }
    *base.add(pos) = v as u8;
    buf.set_len(pos + 1);
}
```

**Invariant:** `buf.reserve(10)` guarantees at least 10 bytes of spare capacity. A varint is at most 10 bytes, so `pos` never exceeds `buf.capacity()`.

**Why unsafe:** Avoids per-byte bounds checks in `Vec::push()`. For multi-byte varints (values ≥ 128), this writes 2-10 bytes with a single `set_len` at the end.

### 2. `tw_encode_packed_u32` — batch varint write (line ~177)

```rust
buf.reserve(values.len() * 5);
unsafe {
    let base = buf.as_mut_ptr();
    for &val in values { /* write varint bytes */ }
    buf.set_len(pos);
}
```

**Invariant:** `reserve(len * 5)` guarantees worst-case capacity (u32 varints are at most 5 bytes each). The loop cannot write more than `len * 5` bytes total.

**Why unsafe:** One `set_len` for the entire batch instead of per-element `push` calls. Eliminates N-1 stores to the Vec length field.

### 3. `tw_encode_varint_unchecked` — pre-reserved encode (line ~204)

```rust
pub unsafe fn tw_encode_varint_unchecked(buf: &mut Vec<u8>, mut value: u64) {
    let base = buf.as_mut_ptr();
    while value >= 0x80 { *base.add(pos) = ...; pos += 1; }
    *base.add(pos) = value as u8;
    buf.set_len(pos + 1);
}
```

**Invariant:** The **caller** must ensure at least 10 bytes of spare capacity. Used only from generated packed encode loops that pre-reserve.

**Why unsafe:** The function itself is `unsafe` — it has no reserve call, relying on the caller's guarantee. Used in hot packed encode loops where a single reserve covers all elements.

### 4. `tw_varint_len` — NonZeroU64 construction (line ~227)

```rust
let log2 = unsafe { core::num::NonZeroU64::new_unchecked(value | 1) }.ilog2();
```

**Invariant:** `value | 1` is always non-zero (the OR with 1 guarantees at least bit 0 is set).

**Why unsafe:** `NonZeroU64::new_unchecked` skips the zero check. The `| 1` makes this unconditionally safe.

## typeway-protobuf/src/lib.rs (BytesStr)

### 5. `BytesStr::from_utf8_unchecked` (line ~84)

```rust
pub unsafe fn from_utf8_unchecked(bytes: Bytes) -> Self {
    BytesStr { inner: bytes }
}
```

**Invariant:** The **caller** must guarantee the bytes are valid UTF-8. The `Deref<Target = str>` impl uses `str::from_utf8_unchecked` which would be UB if the bytes aren't valid UTF-8.

**Why unsafe:** Performance-critical construction path for zero-copy decode. Called from generated `typeway_decode_bytes` after UTF-8 validation on the source slice.

### 6. `BytesStr::deref` (line ~126)

```rust
unsafe { std::str::from_utf8_unchecked(&self.inner) }
```

**Invariant:** All construction paths validate UTF-8 (`from_utf8` checks, `from_utf8_unchecked` requires caller guarantee, `From<String>` is inherently valid, `From<&'static str>` is inherently valid).

**Why unsafe:** Avoids redundant UTF-8 re-validation on every `Deref` call. The type's constructor guarantees validity.

## typeway-macros/src/lib.rs (generated code)

### 7. `String::from_utf8_unchecked` in decode arms

```rust
let slice = &bytes[offset..offset + str_len];
::core::str::from_utf8(slice).map_err(|_| ...)?;
#ident = unsafe { String::from_utf8_unchecked(slice.to_vec()) };
```

**Invariant:** `str::from_utf8(slice)` validates UTF-8 on the line above. The `to_vec()` copies the same bytes, so they're still valid UTF-8.

**Why unsafe:** `String::from_utf8(vec)` would re-validate the bytes we just validated. Using `from_utf8_unchecked` skips the redundant O(n) scan. This saves one full pass over the string data.

**Appears in:** LenString decode, optional string decode, repeated string decode, LenBytesStr decode.

### 8. `BytesStr::from_utf8_unchecked` in `typeway_decode_bytes`

```rust
::core::str::from_utf8(&bytes[offset..offset + str_len]).map_err(|_| ...)?;
#ident = unsafe {
    ::typeway_protobuf::BytesStr::from_utf8_unchecked(input.slice(offset..offset + str_len))
};
```

**Invariant:** Same as #7 — UTF-8 validated on the borrowed slice, then the `Bytes::slice()` references the same data (refcount increment, no copy).

**Why unsafe:** True zero-copy: no allocation, no copy. The `Bytes::slice()` shares the backing buffer. Validation happened on the same bytes.

### 9. Packed varint batch write in generated code

```rust
unsafe {
    let base = buf.as_mut_ptr();
    let mut pos = data_start;
    for item in &self.values {
        let mut v = *item as u64;
        while v >= 0x80 { *base.add(pos) = ...; pos += 1; }
        *base.add(pos) = v as u8;
        pos += 1;
    }
    buf.set_len(pos);
}
```

**Invariant:** `buf.reserve(self.values.len() * 10)` is called before this block. Each varint writes at most 10 bytes. Total writes ≤ `len * 10` ≤ reserved capacity.

**Why unsafe:** One `set_len` for the entire packed field instead of per-element. Eliminates N function calls and N-1 length stores.

### 10. Bulk memcpy for packed f64/f32 in generated code

```rust
#[cfg(target_endian = "little")]
{
    let slice_bytes = unsafe {
        ::core::slice::from_raw_parts(
            self.values.as_ptr() as *const u8,
            self.values.len() * 8,
        )
    };
    buf.extend_from_slice(slice_bytes);
}
```

**Invariant:** `Vec<f64>` is a contiguous allocation of `len * 8` bytes. On little-endian architectures, IEEE 754 f64 representation matches protobuf wire order (little-endian fixed64). The cast from `*const f64` to `*const u8` with length `len * 8` is valid because f64 has no padding and the alignment of u8 (1) divides the alignment of f64 (8).

**Why unsafe:** Single memcpy for the entire Vec instead of per-element `to_le_bytes()` + `extend_from_slice`. For 20 f64 values, this is 1 operation instead of 20.

**Guarded by:** `#[cfg(target_endian = "little")]`. On big-endian architectures, falls back to per-element `to_le_bytes()`.

## Summary

| # | Location | Purpose | Risk |
|---|----------|---------|------|
| 1-3 | Varint encode | Skip Vec bounds checks | Low — reserve guarantees capacity |
| 4 | Varint length | Skip NonZero check | None — `\|1` is always non-zero |
| 5-6 | BytesStr | Skip UTF-8 re-validation | Low — constructors validate |
| 7-8 | String/BytesStr decode | Skip redundant UTF-8 scan | Low — validated on prior line |
| 9 | Packed batch write | One set_len for N elements | Low — reserve covers worst case |
| 10 | Packed f64/f32 memcpy | Bulk write via ptr cast | Low — LE-guarded, no padding |

All unsafe usage follows the pattern: **validate or guarantee the invariant, then use unsafe to skip redundant re-validation or bounds checks**. No unsafe is used for correctness — only for performance.
