//! JWT authentication.

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use typeway_server::error::JsonError;
use typeway_server::extract::FromRequestParts;

const SECRET: &[u8] = b"typeway-realworld-secret-change-in-production";

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub exp: i64,
}

pub fn create_token(user_id: Uuid) -> Result<String, JsonError> {
    let exp = Utc::now() + Duration::hours(24);
    let claims = Claims {
        sub: user_id,
        exp: exp.timestamp(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(SECRET),
    )
    .map_err(|e| JsonError::internal(format!("token creation failed: {e}")))
}

pub fn verify_token(token: &str) -> Result<Claims, JsonError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(SECRET),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| JsonError::unauthorized("invalid or expired token"))
}

/// Extractor: authenticated user ID from JWT in Authorization header.
#[derive(Debug, Clone)]
pub struct AuthUser(pub Uuid);

impl FromRequestParts for AuthUser {
    type Error = JsonError;

    fn from_request_parts(parts: &http::request::Parts) -> Result<Self, Self::Error> {
        let token = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                v.strip_prefix("Token ")
                    .or_else(|| v.strip_prefix("Bearer "))
            })
            .ok_or_else(|| JsonError::unauthorized("missing Authorization header"))?;

        let claims = verify_token(token)?;
        Ok(AuthUser(claims.sub))
    }
}

/// Optional auth — doesn't fail if no token is present.
#[derive(Debug, Clone)]
pub struct OptionalAuth(pub Option<Uuid>);

impl FromRequestParts for OptionalAuth {
    type Error = JsonError;

    fn from_request_parts(parts: &http::request::Parts) -> Result<Self, Self::Error> {
        match AuthUser::from_request_parts(parts) {
            Ok(AuthUser(id)) => Ok(OptionalAuth(Some(id))),
            Err(_) => Ok(OptionalAuth(None)),
        }
    }
}
