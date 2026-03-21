//! Benchmark: protobuf codec encode/decode performance.
//!
//! Compares three approaches:
//! 1. Hand-written codec (json_to_proto_binary / proto_binary_to_json)
//! 2. TypewayCodec (compile-time specialized via derive macro)
//! 3. Prost (industry-standard protobuf library for Rust)
//!
//! Run with: `cargo bench --bench codec --features grpc`

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use prost::Message;
use typeway_grpc::proto_codec::{json_to_proto_binary, proto_binary_to_json, ProtoFieldDef};
use typeway_protobuf::{TypewayDecode, TypewayEncode};
use typeway_macros::TypewayCodec;

// ---------------------------------------------------------------------------
// Test message types
// ---------------------------------------------------------------------------

/// Small message (~20 bytes encoded): typical ID lookup request.
#[derive(Debug, Clone, Default, PartialEq, TypewayCodec)]
struct SmallMessage {
    #[proto(tag = 1)]
    id: u32,
    #[proto(tag = 2)]
    name: String,
}

/// Medium message (~200 bytes encoded): typical API response.
#[derive(Debug, Clone, Default, PartialEq, TypewayCodec)]
struct MediumMessage {
    #[proto(tag = 1)]
    id: u64,
    #[proto(tag = 2)]
    username: String,
    #[proto(tag = 3)]
    email: String,
    #[proto(tag = 4)]
    bio: String,
    #[proto(tag = 5)]
    active: bool,
    #[proto(tag = 6)]
    score: f64,
    #[proto(tag = 7)]
    level: u32,
}

/// Large message (~1KB encoded): nested with repeated fields.
#[derive(Debug, Clone, Default, PartialEq, TypewayCodec)]
struct Tag {
    #[proto(tag = 1)]
    name: String,
    #[proto(tag = 2)]
    value: String,
}

#[derive(Debug, Clone, Default, PartialEq, TypewayCodec)]
struct LargeMessage {
    #[proto(tag = 1)]
    id: u64,
    #[proto(tag = 2)]
    title: String,
    #[proto(tag = 3)]
    body: String,
    #[proto(tag = 4)]
    author: String,
    #[proto(tag = 5)]
    tags: Vec<String>,
    #[proto(tag = 6)]
    view_count: u64,
    #[proto(tag = 7)]
    favorited: bool,
    #[proto(tag = 8)]
    created_at: String,
    #[proto(tag = 9)]
    updated_at: String,
}

// ---------------------------------------------------------------------------
// Prost equivalent types (for direct comparison)
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, prost::Message)]
struct ProstSmallMessage {
    #[prost(uint32, tag = "1")]
    id: u32,
    #[prost(string, tag = "2")]
    name: String,
}

#[derive(Clone, PartialEq, prost::Message)]
struct ProstMediumMessage {
    #[prost(uint64, tag = "1")]
    id: u64,
    #[prost(string, tag = "2")]
    username: String,
    #[prost(string, tag = "3")]
    email: String,
    #[prost(string, tag = "4")]
    bio: String,
    #[prost(bool, tag = "5")]
    active: bool,
    #[prost(double, tag = "6")]
    score: f64,
    #[prost(uint32, tag = "7")]
    level: u32,
}

#[derive(Clone, PartialEq, prost::Message)]
struct ProstLargeMessage {
    #[prost(uint64, tag = "1")]
    id: u64,
    #[prost(string, tag = "2")]
    title: String,
    #[prost(string, tag = "3")]
    body: String,
    #[prost(string, tag = "4")]
    author: String,
    #[prost(string, repeated, tag = "5")]
    tags: Vec<String>,
    #[prost(uint64, tag = "6")]
    view_count: u64,
    #[prost(bool, tag = "7")]
    favorited: bool,
    #[prost(string, tag = "8")]
    created_at: String,
    #[prost(string, tag = "9")]
    updated_at: String,
}

// ---------------------------------------------------------------------------
// Test data constructors
// ---------------------------------------------------------------------------

fn small_msg() -> SmallMessage {
    SmallMessage {
        id: 42,
        name: "Alice".into(),
    }
}

fn small_json() -> serde_json::Value {
    serde_json::json!({"id": 42, "name": "Alice"})
}

fn small_fields() -> Vec<ProtoFieldDef> {
    vec![
        ProtoFieldDef {
            name: "id".into(),
            proto_type: "uint32".into(),
            tag: 1,
            repeated: false,
            is_map: false,
            map_key_type: None,
            map_value_type: None,
            nested_fields: None,
        },
        ProtoFieldDef {
            name: "name".into(),
            proto_type: "string".into(),
            tag: 2,
            repeated: false,
            is_map: false,
            map_key_type: None,
            map_value_type: None,
            nested_fields: None,
        },
    ]
}

fn medium_msg() -> MediumMessage {
    MediumMessage {
        id: 12345,
        username: "johndoe".into(),
        email: "john.doe@example.com".into(),
        bio: "Software developer with 10 years of experience in systems programming.".into(),
        active: true,
        score: 98.5,
        level: 42,
    }
}

fn medium_json() -> serde_json::Value {
    serde_json::json!({
        "id": 12345u64,
        "username": "johndoe",
        "email": "john.doe@example.com",
        "bio": "Software developer with 10 years of experience in systems programming.",
        "active": true,
        "score": 98.5,
        "level": 42
    })
}

fn medium_fields() -> Vec<ProtoFieldDef> {
    vec![
        ProtoFieldDef { name: "id".into(), proto_type: "uint64".into(), tag: 1, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "username".into(), proto_type: "string".into(), tag: 2, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "email".into(), proto_type: "string".into(), tag: 3, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "bio".into(), proto_type: "string".into(), tag: 4, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "active".into(), proto_type: "bool".into(), tag: 5, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "score".into(), proto_type: "double".into(), tag: 6, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "level".into(), proto_type: "uint32".into(), tag: 7, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
    ]
}

fn large_msg() -> LargeMessage {
    LargeMessage {
        id: 999999,
        title: "How to Build a Type-Level Web Framework in Rust".into(),
        body: "This is a comprehensive guide to building a type-level web framework \
               using Rust's type system. We cover HLists, type-level programming, \
               compile-time route validation, and more. The framework interprets API \
               types into servers, clients, and documentation automatically."
            .into(),
        author: "typeway-team".into(),
        tags: vec![
            "rust".into(),
            "web".into(),
            "type-level".into(),
            "framework".into(),
            "grpc".into(),
        ],
        view_count: 42000,
        favorited: true,
        created_at: "2025-01-15T10:30:00Z".into(),
        updated_at: "2025-03-20T14:22:00Z".into(),
    }
}

fn large_json() -> serde_json::Value {
    serde_json::json!({
        "id": 999999u64,
        "title": "How to Build a Type-Level Web Framework in Rust",
        "body": "This is a comprehensive guide to building a type-level web framework \
                 using Rust's type system. We cover HLists, type-level programming, \
                 compile-time route validation, and more. The framework interprets API \
                 types into servers, clients, and documentation automatically.",
        "author": "typeway-team",
        "tags": ["rust", "web", "type-level", "framework", "grpc"],
        "view_count": 42000u64,
        "favorited": true,
        "created_at": "2025-01-15T10:30:00Z",
        "updated_at": "2025-03-20T14:22:00Z"
    })
}

fn large_fields() -> Vec<ProtoFieldDef> {
    vec![
        ProtoFieldDef { name: "id".into(), proto_type: "uint64".into(), tag: 1, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "title".into(), proto_type: "string".into(), tag: 2, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "body".into(), proto_type: "string".into(), tag: 3, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "author".into(), proto_type: "string".into(), tag: 4, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "tags".into(), proto_type: "string".into(), tag: 5, repeated: true, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "view_count".into(), proto_type: "uint64".into(), tag: 6, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "favorited".into(), proto_type: "bool".into(), tag: 7, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "created_at".into(), proto_type: "string".into(), tag: 8, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
        ProtoFieldDef { name: "updated_at".into(), proto_type: "string".into(), tag: 9, repeated: false, is_map: false, map_key_type: None, map_value_type: None, nested_fields: None },
    ]
}

// ---------------------------------------------------------------------------
// Prost data constructors
// ---------------------------------------------------------------------------

fn prost_small_msg() -> ProstSmallMessage {
    ProstSmallMessage {
        id: 42,
        name: "Alice".into(),
    }
}

fn prost_medium_msg() -> ProstMediumMessage {
    ProstMediumMessage {
        id: 12345,
        username: "johndoe".into(),
        email: "john.doe@example.com".into(),
        bio: "Software developer with 10 years of experience in systems programming.".into(),
        active: true,
        score: 98.5,
        level: 42,
    }
}

fn prost_large_msg() -> ProstLargeMessage {
    ProstLargeMessage {
        id: 999999,
        title: "How to Build a Type-Level Web Framework in Rust".into(),
        body: "This is a comprehensive guide to building a type-level web framework \
               using Rust's type system. We cover HLists, type-level programming, \
               compile-time route validation, and more. The framework interprets API \
               types into servers, clients, and documentation automatically."
            .into(),
        author: "typeway-team".into(),
        tags: vec![
            "rust".into(),
            "web".into(),
            "type-level".into(),
            "framework".into(),
            "grpc".into(),
        ],
        view_count: 42000,
        favorited: true,
        created_at: "2025-01-15T10:30:00Z".into(),
        updated_at: "2025-03-20T14:22:00Z".into(),
    }
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode");

    // Small message
    let small = small_msg();
    let small_j = small_json();
    let small_f = small_fields();

    group.bench_function("small/typeway_codec", |b| {
        b.iter(|| black_box(small.encode_to_vec()))
    });
    group.bench_function("small/hand_written", |b| {
        b.iter(|| black_box(json_to_proto_binary(&small_j, &small_f).unwrap()))
    });
    let prost_small = prost_small_msg();
    group.bench_function("small/prost", |b| {
        b.iter(|| black_box(prost_small.encode_to_vec()))
    });

    // Medium message
    let medium = medium_msg();
    let medium_j = medium_json();
    let medium_f = medium_fields();

    group.bench_function("medium/typeway_codec", |b| {
        b.iter(|| black_box(medium.encode_to_vec()))
    });
    group.bench_function("medium/hand_written", |b| {
        b.iter(|| black_box(json_to_proto_binary(&medium_j, &medium_f).unwrap()))
    });
    let prost_medium = prost_medium_msg();
    group.bench_function("medium/prost", |b| {
        b.iter(|| black_box(prost_medium.encode_to_vec()))
    });

    // Large message
    let large = large_msg();
    let large_j = large_json();
    let large_f = large_fields();

    group.bench_function("large/typeway_codec", |b| {
        b.iter(|| black_box(large.encode_to_vec()))
    });
    group.bench_function("large/hand_written", |b| {
        b.iter(|| black_box(json_to_proto_binary(&large_j, &large_f).unwrap()))
    });
    let prost_large = prost_large_msg();
    group.bench_function("large/prost", |b| {
        b.iter(|| black_box(prost_large.encode_to_vec()))
    });

    group.finish();
}

fn bench_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode");

    // Small message
    let small_bytes = small_msg().encode_to_vec();
    let small_f = small_fields();

    group.bench_function("small/typeway_codec", |b| {
        b.iter(|| black_box(SmallMessage::typeway_decode(&small_bytes).unwrap()))
    });
    group.bench_function("small/hand_written", |b| {
        b.iter(|| black_box(proto_binary_to_json(&small_bytes, &small_f).unwrap()))
    });
    let small_bytes_b = bytes::Bytes::from(small_bytes.clone());
    group.bench_function("small/typeway_zerocopy", |b| {
        b.iter(|| {
            let input = small_bytes_b.clone();
            black_box(SmallMessage::typeway_decode_bytes(input).unwrap())
        })
    });
    group.bench_function("small/prost", |b| {
        b.iter(|| black_box(ProstSmallMessage::decode(small_bytes.as_slice()).unwrap()))
    });

    // Medium message
    let medium_bytes = medium_msg().encode_to_vec();
    let medium_f = medium_fields();

    group.bench_function("medium/typeway_codec", |b| {
        b.iter(|| black_box(MediumMessage::typeway_decode(&medium_bytes).unwrap()))
    });
    group.bench_function("medium/hand_written", |b| {
        b.iter(|| black_box(proto_binary_to_json(&medium_bytes, &medium_f).unwrap()))
    });
    let medium_bytes_b = bytes::Bytes::from(medium_bytes.clone());
    group.bench_function("medium/typeway_zerocopy", |b| {
        b.iter(|| {
            let input = medium_bytes_b.clone();
            black_box(MediumMessage::typeway_decode_bytes(input).unwrap())
        })
    });
    group.bench_function("medium/prost", |b| {
        b.iter(|| black_box(ProstMediumMessage::decode(medium_bytes.as_slice()).unwrap()))
    });

    // Large message
    let large_bytes = large_msg().encode_to_vec();
    let large_f = large_fields();

    group.bench_function("large/typeway_codec", |b| {
        b.iter(|| black_box(LargeMessage::typeway_decode(&large_bytes).unwrap()))
    });
    group.bench_function("large/hand_written", |b| {
        b.iter(|| black_box(proto_binary_to_json(&large_bytes, &large_f).unwrap()))
    });
    let large_bytes_b = bytes::Bytes::from(large_bytes.clone());
    group.bench_function("large/typeway_zerocopy", |b| {
        b.iter(|| {
            let input = large_bytes_b.clone();
            black_box(LargeMessage::typeway_decode_bytes(input).unwrap())
        })
    });
    group.bench_function("large/prost", |b| {
        b.iter(|| black_box(ProstLargeMessage::decode(large_bytes.as_slice()).unwrap()))
    });

    group.finish();
}

fn bench_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("roundtrip");

    // Small
    let small = small_msg();
    group.bench_function("small/typeway_codec", |b| {
        b.iter(|| {
            let encoded = small.encode_to_vec();
            let decoded = SmallMessage::typeway_decode(&encoded).unwrap();
            black_box(decoded)
        })
    });

    // Medium
    let medium = medium_msg();
    group.bench_function("medium/typeway_codec", |b| {
        b.iter(|| {
            let encoded = medium.encode_to_vec();
            let decoded = MediumMessage::typeway_decode(&encoded).unwrap();
            black_box(decoded)
        })
    });

    // Large
    let large = large_msg();
    group.bench_function("large/typeway_codec", |b| {
        b.iter(|| {
            let encoded = large.encode_to_vec();
            let decoded = LargeMessage::typeway_decode(&encoded).unwrap();
            black_box(decoded)
        })
    });

    // Prost roundtrips
    let prost_small = prost_small_msg();
    group.bench_function("small/prost", |b| {
        b.iter(|| {
            let encoded = prost_small.encode_to_vec();
            let decoded = ProstSmallMessage::decode(encoded.as_slice()).unwrap();
            black_box(decoded)
        })
    });

    let prost_medium = prost_medium_msg();
    group.bench_function("medium/prost", |b| {
        b.iter(|| {
            let encoded = prost_medium.encode_to_vec();
            let decoded = ProstMediumMessage::decode(encoded.as_slice()).unwrap();
            black_box(decoded)
        })
    });

    let prost_large = prost_large_msg();
    group.bench_function("large/prost", |b| {
        b.iter(|| {
            let encoded = prost_large.encode_to_vec();
            let decoded = ProstLargeMessage::decode(encoded.as_slice()).unwrap();
            black_box(decoded)
        })
    });

    group.finish();
}

criterion_group!(benches, bench_encode, bench_decode, bench_roundtrip);
criterion_main!(benches);
