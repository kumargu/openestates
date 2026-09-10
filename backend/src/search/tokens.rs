use std::sync::OnceLock;

use hmac::{Hmac, KeyInit, Mac};
use serde::de::DeserializeOwned;
use serde::Serialize;
use sha2::{Digest, Sha256};

const SEARCH_SIGNING_KEY_ENV: &str = "OPENESTATES_REVISION_SIGNING_KEY";
const TOKEN_VERSION: &str = "v1";
type HmacSha256 = Hmac<Sha256>;

pub(super) fn encode_signed<T: Serialize>(
    purpose: &str,
    value: &T,
    max_encoded_bytes: usize,
) -> Result<String, String> {
    let payload = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let signature = tagged_hmac(purpose, &payload);
    let encoded = format!(
        "{TOKEN_VERSION}.{}.{}",
        encode_hex(&payload),
        encode_hex(&signature)
    );
    if encoded.len() > max_encoded_bytes {
        return Err("signed token exceeds the encoded size limit".to_string());
    }
    Ok(encoded)
}

pub(super) fn decode_signed<T: DeserializeOwned>(
    purpose: &str,
    encoded: &str,
    max_encoded_bytes: usize,
) -> Result<T, String> {
    if encoded.len() > max_encoded_bytes {
        return Err("signed token exceeds the encoded size limit".to_string());
    }
    let mut parts = encoded.split('.');
    if parts.next() != Some(TOKEN_VERSION) {
        return Err("unsupported signed token version".to_string());
    }
    let payload = decode_hex(
        parts
            .next()
            .ok_or_else(|| "missing signed token payload".to_string())?,
    )?;
    let signature = decode_hex(
        parts
            .next()
            .ok_or_else(|| "missing signed token signature".to_string())?,
    )?;
    if parts.next().is_some() {
        return Err("invalid signed token framing".to_string());
    }
    verify_tagged_hmac(purpose, &payload, &signature)?;
    serde_json::from_slice(&payload)
        .map_err(|error| format!("invalid signed token payload: {error}"))
}

pub(super) fn tagged_digest(purpose: &str, payload: &[u8]) -> [u8; 32] {
    tagged_hmac(purpose, payload)
}

pub(super) fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("invalid signed token encoding".to_string());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| "invalid signed token encoding".to_string())
        })
        .collect()
}

fn tagged_hmac(purpose: &str, payload: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(search_signing_key())
        .expect("HMAC accepts arbitrary key lengths");
    mac.update(purpose.as_bytes());
    mac.update(&[0]);
    mac.update(payload);
    mac.finalize().into_bytes().into()
}

fn verify_tagged_hmac(purpose: &str, payload: &[u8], signature: &[u8]) -> Result<(), String> {
    let mut mac = HmacSha256::new_from_slice(search_signing_key())
        .expect("HMAC accepts arbitrary key lengths");
    mac.update(purpose.as_bytes());
    mac.update(&[0]);
    mac.update(payload);
    mac.verify_slice(signature)
        .map_err(|_| "invalid signed token signature".to_string())
}

fn search_signing_key() -> &'static [u8; 32] {
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    KEY.get_or_init(|| {
        if let Ok(configured) = std::env::var(SEARCH_SIGNING_KEY_ENV) {
            if !configured.trim().is_empty() {
                return Sha256::digest(configured.as_bytes()).into();
            }
        }
        if cfg!(debug_assertions) {
            return Sha256::digest(b"openestates-local-revision-signing-key-v1").into();
        }
        panic!("OPENESTATES_REVISION_SIGNING_KEY is required for search token continuity");
    })
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Payload {
        id: String,
    }

    #[test]
    fn purpose_tags_prevent_cross_protocol_token_reuse() {
        let token =
            encode_signed("search-revision-v1", &Payload { id: "one".into() }, 1024).unwrap();
        assert_eq!(
            decode_signed::<Payload>("search-revision-v1", &token, 1024).unwrap(),
            Payload { id: "one".into() }
        );
        assert!(decode_signed::<Payload>("search-proof-v1", &token, 1024).is_err());
    }
}
