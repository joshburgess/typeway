//! The [`client_api!`] macro for generating named client wrapper structs.
//!
//! Instead of calling `client.call::<EndpointType>(args)` with turbofish
//! syntax, this macro generates a wrapper struct with a named method for each
//! endpoint.
//!
//! # Example
//!
//! ```ignore
//! use typeway_client::client_api;
//! use typeway_core::*;
//! use typeway_macros::*;
//!
//! typeway_path!(type UsersPath = "users");
//! typeway_path!(type UserByIdPath = "users" / u32);
//!
//! client_api! {
//!     pub struct UserClient;
//!
//!     /// List all users.
//!     list_users => GetEndpoint<UsersPath, Vec<User>>;
//!
//!     /// Get a user by ID.
//!     get_user => GetEndpoint<UserByIdPath, User>;
//!
//!     /// Create a new user.
//!     create_user => PostEndpoint<UsersPath, CreateUser, User>;
//! }
//!
//! // Use the generated struct:
//! let client = UserClient::new("http://localhost:3000").unwrap();
//! let users = client.list_users(()).await.unwrap();
//! let user = client.get_user((42u32,)).await.unwrap();
//! ```

/// Generate a named client wrapper struct with a method per endpoint.
///
/// See the [module-level documentation](self) for usage examples.
#[macro_export]
macro_rules! client_api {
    // Entry point: parse struct definition + methods.
    (
        $(#[$struct_meta:meta])*
        $vis:vis struct $Name:ident;

        $(
            $(#[$method_meta:meta])*
            $method_name:ident => $Endpoint:ty;
        )*
    ) => {
        $(#[$struct_meta])*
        $vis struct $Name {
            inner: $crate::Client,
        }

        impl $Name {
            /// Create a new client pointing at the given base URL with default
            /// config.
            $vis fn new(base_url: &str) -> ::core::result::Result<Self, $crate::ClientError> {
                ::core::result::Result::Ok(Self {
                    inner: $crate::Client::new(base_url)?,
                })
            }

            /// Create a client with a custom [`ClientConfig`](crate::ClientConfig).
            $vis fn with_config(
                base_url: &str,
                config: $crate::ClientConfig,
            ) -> ::core::result::Result<Self, $crate::ClientError> {
                ::core::result::Result::Ok(Self {
                    inner: $crate::Client::with_config(base_url, config)?,
                })
            }

            /// Create a client wrapping an existing [`Client`](crate::Client).
            $vis fn from_client(client: $crate::Client) -> Self {
                Self { inner: client }
            }

            /// Returns a reference to the inner [`Client`](crate::Client).
            $vis fn inner(&self) -> &$crate::Client {
                &self.inner
            }

            $(
                $(#[$method_meta])*
                $vis async fn $method_name(
                    &self,
                    args: <$Endpoint as $crate::CallEndpoint>::Args,
                ) -> ::core::result::Result<
                    <$Endpoint as $crate::CallEndpoint>::Response,
                    $crate::ClientError,
                > {
                    self.inner.call::<$Endpoint>(args).await
                }
            )*
        }
    };
}
