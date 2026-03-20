use typeway_grpc::status::{GrpcCode, GrpcStatus, IntoGrpcStatus};

#[test]
fn grpc_status_ok() {
    let s = GrpcStatus::ok();
    assert_eq!(s.code, GrpcCode::Ok);
    assert!(s.message.is_empty());
}

#[test]
fn grpc_status_not_found() {
    let s = GrpcStatus::not_found("user 42 not found");
    assert_eq!(s.code, GrpcCode::NotFound);
    assert_eq!(s.message, "user 42 not found");
}

#[test]
fn grpc_status_invalid_argument() {
    let s = GrpcStatus::invalid_argument("missing field: name");
    assert_eq!(s.code, GrpcCode::InvalidArgument);
    assert_eq!(s.message, "missing field: name");
}

#[test]
fn grpc_status_unauthenticated() {
    let s = GrpcStatus::unauthenticated("token expired");
    assert_eq!(s.code, GrpcCode::Unauthenticated);
    assert_eq!(s.message, "token expired");
}

#[test]
fn grpc_status_permission_denied() {
    let s = GrpcStatus::permission_denied("admin only");
    assert_eq!(s.code, GrpcCode::PermissionDenied);
    assert_eq!(s.message, "admin only");
}

#[test]
fn grpc_status_internal() {
    let s = GrpcStatus::internal("unexpected failure");
    assert_eq!(s.code, GrpcCode::Internal);
    assert_eq!(s.message, "unexpected failure");
}

#[test]
fn grpc_status_unimplemented() {
    let s = GrpcStatus::unimplemented("not yet");
    assert_eq!(s.code, GrpcCode::Unimplemented);
    assert_eq!(s.message, "not yet");
}

#[test]
fn grpc_status_unavailable() {
    let s = GrpcStatus::unavailable("try later");
    assert_eq!(s.code, GrpcCode::Unavailable);
    assert_eq!(s.message, "try later");
}

#[test]
fn grpc_status_already_exists() {
    let s = GrpcStatus::already_exists("duplicate key");
    assert_eq!(s.code, GrpcCode::AlreadyExists);
    assert_eq!(s.message, "duplicate key");
}

#[test]
fn grpc_status_resource_exhausted() {
    let s = GrpcStatus::resource_exhausted("rate limited");
    assert_eq!(s.code, GrpcCode::ResourceExhausted);
    assert_eq!(s.message, "rate limited");
}

#[test]
fn to_headers_ok() {
    let s = GrpcStatus::ok();
    let headers = s.to_headers();
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0], ("grpc-status".to_string(), "0".to_string()));
}

#[test]
fn to_headers_with_message() {
    let s = GrpcStatus::not_found("gone");
    let headers = s.to_headers();
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[0], ("grpc-status".to_string(), "5".to_string()));
    assert_eq!(headers[1], ("grpc-message".to_string(), "gone".to_string()));
}

#[test]
fn into_grpc_status_for_http_status_code() {
    let status = http::StatusCode::NOT_FOUND;
    let grpc = status.into_grpc_status();
    assert_eq!(grpc.code, GrpcCode::NotFound);
    assert_eq!(grpc.message, "Not Found");
}

#[test]
fn into_grpc_status_for_http_ok() {
    let status = http::StatusCode::OK;
    let grpc = status.into_grpc_status();
    assert_eq!(grpc.code, GrpcCode::Ok);
    assert_eq!(grpc.message, "OK");
}

#[test]
fn into_grpc_status_for_http_500() {
    let status = http::StatusCode::INTERNAL_SERVER_ERROR;
    let grpc = status.into_grpc_status();
    assert_eq!(grpc.code, GrpcCode::Internal);
    assert_eq!(grpc.message, "Internal Server Error");
}
