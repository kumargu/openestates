use std::collections::BTreeMap;
use std::sync::OnceLock;

use hmac::{Hmac, KeyInit, Mac};
use serde::de::DeserializeOwned;
use serde::Serialize;
use sha2::{Digest, Sha256};

const SEARCH_SIGNING_CURRENT_KID_ENV: &str = "OPENESTATES_SEARCH_SIGNING_CURRENT_KID";
const SEARCH_SIGNING_KEYS_ENV: &str = "OPENESTATES_SEARCH_SIGNING_KEYS";
const TOKEN_VERSION: &str = "v2";
type HmacSha256 = Hmac<Sha256>;

#[derive(Debug)]
struct SearchSigningKeyRing {
    current_kid: String,
    keys: BTreeMap<String, [u8; 32]>,
}

impl SearchSigningKeyRing {
    fn from_secrets(
        current_kid: String,
        secrets: BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let tuning = &crate::security::security_tuning().search_journey;
        if current_kid.trim().is_empty()
            || current_kid.len() > tuning.max_signing_key_id_bytes
            || !valid_key_id(&current_kid)
        {
            return Err("current search signing key id is invalid".to_string());
        }
        if secrets.is_empty() || secrets.len() > tuning.max_signing_keys {
            return Err("search signing key ring has an invalid size".to_string());
        }
        let mut keys = BTreeMap::new();
        for (kid, secret) in secrets {
            if kid.trim().is_empty()
                || kid.len() > tuning.max_signing_key_id_bytes
                || !valid_key_id(&kid)
                || secret.trim().is_empty()
            {
                return Err("search signing key ring contains an invalid entry".to_string());
            }
            keys.insert(kid, Sha256::digest(secret.as_bytes()).into());
        }
        if !keys.contains_key(&current_kid) {
            return Err("current search signing key id is absent from the key ring".to_string());
        }
        Ok(Self { current_kid, keys })
    }

    fn current_key(&self) -> &[u8; 32] {
        self.keys
            .get(&self.current_kid)
            .expect("validated current signing key exists")
    }
}

pub(super) fn encode_signed<T: Serialize>(
    purpose: &str,
    value: &T,
    max_encoded_bytes: usize,
) -> Result<String, String> {
    encode_signed_with_ring(purpose, value, max_encoded_bytes, search_signing_keys())
}

fn encode_signed_with_ring<T: Serialize>(
    purpose: &str,
    value: &T,
    max_encoded_bytes: usize,
    key_ring: &SearchSigningKeyRing,
) -> Result<String, String> {
    let payload = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let signature = tagged_hmac(
        key_ring.current_key(),
        purpose,
        &key_ring.current_kid,
        &payload,
    );
    let encoded = format!(
        "{TOKEN_VERSION}.{}.{}.{}",
        key_ring.current_kid,
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
    decode_signed_with_ring(purpose, encoded, max_encoded_bytes, search_signing_keys())
}

fn decode_signed_with_ring<T: DeserializeOwned>(
    purpose: &str,
    encoded: &str,
    max_encoded_bytes: usize,
    key_ring: &SearchSigningKeyRing,
) -> Result<T, String> {
    if encoded.len() > max_encoded_bytes {
        return Err("signed token exceeds the encoded size limit".to_string());
    }
    let mut parts = encoded.split('.');
    if parts.next() != Some(TOKEN_VERSION) {
        return Err("unsupported signed token version".to_string());
    }
    let kid = parts
        .next()
        .ok_or_else(|| "missing signed token key id".to_string())?;
    if !valid_key_id(kid) {
        return Err("invalid signed token key id".to_string());
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
    let key = key_ring
        .keys
        .get(kid)
        .ok_or_else(|| "unknown signed token key id".to_string())?;
    verify_tagged_hmac(key, purpose, kid, &payload, &signature)?;
    serde_json::from_slice(&payload)
        .map_err(|error| format!("invalid signed token payload: {error}"))
}

pub(super) fn tagged_digest(purpose: &str, payload: &[u8]) -> [u8; 32] {
    let key_ring = search_signing_keys();
    tagged_hmac(
        key_ring.current_key(),
        purpose,
        &key_ring.current_kid,
        payload,
    )
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

fn tagged_hmac(key: &[u8; 32], purpose: &str, kid: &str, payload: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts arbitrary key lengths");
    mac.update(purpose.as_bytes());
    mac.update(&[0]);
    mac.update(kid.as_bytes());
    mac.update(&[0]);
    mac.update(payload);
    mac.finalize().into_bytes().into()
}

fn verify_tagged_hmac(
    key: &[u8; 32],
    purpose: &str,
    kid: &str,
    payload: &[u8],
    signature: &[u8],
) -> Result<(), String> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts arbitrary key lengths");
    mac.update(purpose.as_bytes());
    mac.update(&[0]);
    mac.update(kid.as_bytes());
    mac.update(&[0]);
    mac.update(payload);
    mac.verify_slice(signature)
        .map_err(|_| "invalid signed token signature".to_string())
}

fn valid_key_id(kid: &str) -> bool {
    !kid.is_empty()
        && kid
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn search_signing_keys() -> &'static SearchSigningKeyRing {
    static KEYS: OnceLock<SearchSigningKeyRing> = OnceLock::new();
    KEYS.get_or_init(|| {
        let current_kid = std::env::var(SEARCH_SIGNING_CURRENT_KID_ENV).ok();
        let secrets = std::env::var(SEARCH_SIGNING_KEYS_ENV).ok().map(|encoded| {
            serde_json::from_str::<BTreeMap<String, String>>(&encoded)
                .unwrap_or_else(|error| panic!("invalid {SEARCH_SIGNING_KEYS_ENV}: {error}"))
        });
        match (current_kid, secrets) {
            (Some(current_kid), Some(secrets)) => {
                SearchSigningKeyRing::from_secrets(current_kid, secrets)
                    .unwrap_or_else(|error| panic!("invalid search signing key ring: {error}"))
            }
            (None, None) if cfg!(debug_assertions) => SearchSigningKeyRing::from_secrets(
                "local-v1".to_string(),
                BTreeMap::from([(
                    "local-v1".to_string(),
                    "openestates-local-search-signing-key-v1".to_string(),
                )]),
            )
            .expect("local search signing key ring is valid"),
            _ => panic!(
                "{SEARCH_SIGNING_CURRENT_KID_ENV} and {SEARCH_SIGNING_KEYS_ENV} must be configured together"
            ),
        }
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

    fn ring(current: &str) -> SearchSigningKeyRing {
        SearchSigningKeyRing::from_secrets(
            current.to_string(),
            BTreeMap::from([
                ("current".to_string(), "current-secret".to_string()),
                ("previous".to_string(), "previous-secret".to_string()),
            ]),
        )
        .unwrap()
    }

    #[test]
    fn purpose_tags_prevent_cross_protocol_token_reuse() {
        let keys = ring("current");
        let token = encode_signed_with_ring(
            "search-revision-v1",
            &Payload { id: "one".into() },
            1024,
            &keys,
        )
        .unwrap();
        assert_eq!(
            decode_signed_with_ring::<Payload>("search-revision-v1", &token, 1024, &keys).unwrap(),
            Payload { id: "one".into() }
        );
        assert!(
            decode_signed_with_ring::<Payload>("search-proof-v1", &token, 1024, &keys).is_err()
        );
    }

    #[test]
    fn previous_key_tokens_survive_rotation_and_new_tokens_use_current_kid() {
        let previous_current = ring("previous");
        let token = encode_signed_with_ring(
            "search-revision-v1",
            &Payload { id: "saved".into() },
            1024,
            &previous_current,
        )
        .unwrap();
        assert!(token.starts_with("v2.previous."));

        let rotated = ring("current");
        assert_eq!(
            decode_signed_with_ring::<Payload>("search-revision-v1", &token, 1024, &rotated)
                .unwrap(),
            Payload { id: "saved".into() }
        );
        let new_token = encode_signed_with_ring(
            "search-revision-v1",
            &Payload { id: "new".into() },
            1024,
            &rotated,
        )
        .unwrap();
        assert!(new_token.starts_with("v2.current."));
    }
}
