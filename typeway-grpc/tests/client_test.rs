#![allow(dead_code)]
//! Compile-time tests for the `grpc_client!` macro.
//!
//! These tests verify that the macro expands correctly and produces a struct
//! with the expected methods. Actual network calls are not tested here.

#[cfg(feature = "client")]
mod client_tests {
    use typeway_core::endpoint::GetEndpoint;

    // Define a simple path type for testing.
    typeway_macros::typeway_path!(type TestPath = "test");

    // A simple response type.
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct TestResponse {
        message: String,
    }

    type TestEndpoint = GetEndpoint<TestPath, TestResponse>;

    typeway_grpc::grpc_client! {
        /// A test gRPC client.
        pub struct TestGrpcClient;
        service = "TestService";
        package = "test.v1";

        /// Get a test resource.
        get_test => TestEndpoint;
    }

    #[test]
    fn client_struct_compiles() {
        // This test verifies that the macro expansion compiles.
        let _: fn(&str) -> Result<TestGrpcClient, typeway_grpc::client::GrpcClientError> =
            TestGrpcClient::new;
    }

    #[test]
    fn service_path_is_correct() {
        let client = TestGrpcClient::new("http://localhost:3000").unwrap();
        assert_eq!(client.service_path(), "test.v1.TestService");
    }
}
