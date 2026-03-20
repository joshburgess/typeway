use typeway_grpc::ToProtoType;
use typeway_macros::ToProtoType;

#[derive(ToProtoType)]
struct Point {
    x: f64,
    y: f64,
    z: f64,
}

fn main() {
    let def = Point::message_definition().unwrap();
    assert!(def.contains("double x = 1;"), "got: {def}");
    assert!(def.contains("double y = 2;"), "got: {def}");
    assert!(def.contains("double z = 3;"), "got: {def}");
}
