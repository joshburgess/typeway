//! Runtime tests for TypewayCodec encode/decode round-trips.

use typeway_macros::TypewayCodec;
use typeway_protobuf::*;

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default, Debug, PartialEq, Clone)]
struct Simple {
    #[proto(tag = 1)]
    name: String,
    #[proto(tag = 2)]
    age: u32,
    #[proto(tag = 3)]
    active: bool,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default, Debug, PartialEq, Clone)]
struct AllScalars {
    #[proto(tag = 1)]
    u32_val: u32,
    #[proto(tag = 2)]
    u64_val: u64,
    #[proto(tag = 3)]
    i32_val: i32,
    #[proto(tag = 4)]
    i64_val: i64,
    #[proto(tag = 5)]
    f32_val: f32,
    #[proto(tag = 6)]
    f64_val: f64,
    #[proto(tag = 7)]
    bool_val: bool,
    #[proto(tag = 8)]
    str_val: String,
    #[proto(tag = 9)]
    bytes_val: Vec<u8>,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default, Debug, PartialEq, Clone)]
struct WithOptionals {
    #[proto(tag = 1)]
    required: String,
    #[proto(tag = 2)]
    opt_string: Option<String>,
    #[proto(tag = 3)]
    opt_u32: Option<u32>,
    #[proto(tag = 4)]
    opt_bool: Option<bool>,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default, Debug, PartialEq, Clone)]
struct WithRepeated {
    #[proto(tag = 1)]
    tags: Vec<String>,
    #[proto(tag = 2)]
    scores: Vec<u32>,
    #[proto(tag = 3)]
    flags: Vec<bool>,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default, Debug, PartialEq, Clone)]
struct Nested {
    #[proto(tag = 1)]
    inner: Simple,
    #[proto(tag = 2)]
    items: Vec<Simple>,
    #[proto(tag = 3)]
    maybe: Option<Simple>,
}

#[derive(TypewayCodec, serde::Serialize, serde::Deserialize, Default, Debug, Clone)]
struct WithBytesStr {
    #[proto(tag = 1)]
    symbol: BytesStr,
    #[proto(tag = 2)]
    notes: Option<BytesStr>,
    #[proto(tag = 3)]
    tags: Vec<BytesStr>,
}

fn roundtrip<T: TypewayEncode + TypewayDecode + std::fmt::Debug + PartialEq>(val: &T) {
    let encoded = val.encode_to_vec();
    let decoded = T::typeway_decode(&encoded).expect("decode failed");
    assert_eq!(val, &decoded, "roundtrip mismatch");
}

#[test]
fn simple_roundtrip() {
    roundtrip(&Simple { name: "Alice".into(), age: 30, active: true });
}

#[test]
fn simple_default_is_empty_encoding() {
    let val = Simple::default();
    let encoded = val.encode_to_vec();
    assert!(encoded.is_empty(), "default values should encode to empty");
    let decoded = Simple::typeway_decode(&encoded).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn all_scalars_roundtrip() {
    roundtrip(&AllScalars {
        u32_val: 42,
        u64_val: u64::MAX,
        i32_val: -100,
        i64_val: i64::MIN,
        f32_val: 3.14,
        f64_val: 2.718281828,
        bool_val: true,
        str_val: "hello world".into(),
        bytes_val: vec![0x00, 0xFF, 0x42],
    });
}

#[test]
fn scalars_boundary_values() {
    roundtrip(&AllScalars {
        u32_val: u32::MAX,
        u64_val: u64::MAX,
        i32_val: i32::MIN,
        i64_val: i64::MIN,
        f32_val: f32::MAX,
        f64_val: f64::MIN,
        bool_val: false,
        str_val: "x".repeat(10_000),
        bytes_val: vec![0xAB; 1000],
    });
}

#[test]
fn optionals_all_some() {
    roundtrip(&WithOptionals {
        required: "hello".into(),
        opt_string: Some("world".into()),
        opt_u32: Some(99),
        opt_bool: Some(true),
    });
}

#[test]
fn optionals_all_none() {
    roundtrip(&WithOptionals {
        required: "hello".into(),
        opt_string: None,
        opt_u32: None,
        opt_bool: None,
    });
}

#[test]
fn optionals_mixed() {
    roundtrip(&WithOptionals {
        required: "test".into(),
        opt_string: Some("present".into()),
        opt_u32: None,
        // Note: Some(false) encodes identically to None in proto3
        // (false is the default, so it's not written to the wire).
        opt_bool: Some(true),
    });
}

#[test]
fn optional_bool_false_roundtrips() {
    // TypewayCodec encodes Option<bool> = Some(false) explicitly,
    // unlike proto3 which omits default values. This means
    // Some(false) and None are distinguishable.
    roundtrip(&WithOptionals {
        required: "test".into(),
        opt_string: None,
        opt_u32: None,
        opt_bool: Some(false),
    });
}

#[test]
fn repeated_empty() {
    roundtrip(&WithRepeated { tags: vec![], scores: vec![], flags: vec![] });
}

#[test]
fn repeated_populated() {
    roundtrip(&WithRepeated {
        tags: vec!["a".into(), "b".into(), "c".into()],
        scores: vec![1, 2, 3, 100, 999],
        flags: vec![true, false, true],
    });
}

#[test]
fn nested_roundtrip() {
    roundtrip(&Nested {
        inner: Simple { name: "inner".into(), age: 10, active: true },
        items: vec![
            Simple { name: "a".into(), age: 1, active: true },
            Simple { name: "b".into(), age: 2, active: false },
        ],
        maybe: Some(Simple { name: "opt".into(), age: 99, active: true }),
    });
}

#[test]
fn nested_empty_collections() {
    roundtrip(&Nested { inner: Simple::default(), items: vec![], maybe: None });
}

#[test]
fn bytesstr_roundtrip() {
    let val = WithBytesStr {
        symbol: BytesStr::from("AAPL"),
        notes: Some(BytesStr::from("buy signal")),
        tags: vec![BytesStr::from("tech"), BytesStr::from("mega-cap")],
    };
    let encoded = val.encode_to_vec();
    let decoded = WithBytesStr::typeway_decode(&encoded).unwrap();
    assert_eq!(val.symbol.as_bytes(), decoded.symbol.as_bytes());
    assert_eq!(
        val.notes.as_ref().map(|s| s.as_bytes()),
        decoded.notes.as_ref().map(|s| s.as_bytes()),
    );
    assert_eq!(val.tags.len(), decoded.tags.len());
    for (a, b) in val.tags.iter().zip(decoded.tags.iter()) {
        assert_eq!(a.as_bytes(), b.as_bytes());
    }
}

#[test]
fn bytesstr_empty_fields() {
    let val = WithBytesStr { symbol: BytesStr::default(), notes: None, tags: vec![] };
    let encoded = val.encode_to_vec();
    let decoded = WithBytesStr::typeway_decode(&encoded).unwrap();
    assert!(decoded.symbol.is_empty());
    assert!(decoded.notes.is_none());
    assert!(decoded.tags.is_empty());
}

#[test]
fn bytesstr_zero_copy_decode() {
    let val = WithBytesStr { symbol: BytesStr::from("GOOG"), notes: None, tags: vec![] };
    let encoded = val.encode_to_vec();
    let bytes = bytes::Bytes::from(encoded);
    let decoded = WithBytesStr::typeway_decode_bytes(bytes).unwrap();
    assert!(decoded.symbol == "GOOG");
}

#[test]
fn decode_truncated_input_does_not_panic() {
    let val = Simple { name: "test".into(), age: 42, active: true };
    let encoded = val.encode_to_vec();
    for len in 1..encoded.len() {
        let _ = Simple::typeway_decode(&encoded[..len]);
    }
}

#[test]
fn decode_empty_input() {
    let decoded = Simple::typeway_decode(&[]).unwrap();
    assert_eq!(decoded, Simple::default());
}

#[test]
fn encoded_len_matches_actual_simple() {
    let val = Simple { name: "Alice".into(), age: 30, active: true };
    assert_eq!(val.encoded_len(), val.encode_to_vec().len());
}

#[test]
fn encoded_len_matches_actual_all_scalars() {
    let val = AllScalars {
        u32_val: 42, u64_val: 123456789, i32_val: -50, i64_val: -999999,
        f32_val: 1.5, f64_val: 2.5, bool_val: true,
        str_val: "hello".into(), bytes_val: vec![1, 2, 3],
    };
    assert_eq!(val.encoded_len(), val.encode_to_vec().len());
}

#[test]
fn encoded_len_matches_actual_nested() {
    let val = Nested {
        inner: Simple { name: "x".into(), age: 1, active: true },
        items: vec![Simple { name: "a".into(), age: 10, active: false }],
        maybe: Some(Simple { name: "b".into(), age: 20, active: true }),
    };
    assert_eq!(val.encoded_len(), val.encode_to_vec().len());
}
