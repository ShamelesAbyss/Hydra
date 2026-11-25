//! Narrow Hyperliquid order signing boundary. No transfer/withdrawal API exists.
//!
//! MessagePack order and EIP-712 Agent encoding follow the official Python SDK
//! 2fdb18f9517675ea03695a0962bd19eece9c83f0. Cryptographic operations are provided
//! by sha3 and maintained libsecp256k1 bindings, never implemented here.
use anyhow::{ensure, Context, Result};
use secp256k1::{ecdsa::RecoverableSignature, Message, SecretKey};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::{
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};

const MAINNET_EXCHANGE: &str = "https://api.hyperliquid.xyz/exchange";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
static LAST_NONCE: AtomicU64 = AtomicU64::new(0);

pub fn keccak256(bytes: impl AsRef<[u8]>) -> [u8; 32] {
    Keccak256::digest(bytes.as_ref()).into()
}

fn parse_key(value: &str) -> Result<SecretKey> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    ensure!(
        value.len() == 64,
        "invalid signing key; key material omitted"
    );
    SecretKey::from_str(value)
        .map_err(|_| anyhow::anyhow!("invalid signing key; key material omitted"))
}

/// Local public-address derivation; never performs an account probe.
pub fn derive_address(key: &str) -> Result<String> {
    let public = parse_key(key)?.public_key().serialize_uncompressed();
    let digest = keccak256(&public[1..]);
    Ok(format!("0x{}", hex::encode(&digest[12..])))
}

/// Hash a pre-serialized ordered action with the protocol nonce/vault/expiry suffix.
/// The production order builder supplies no vault or expiry; these parameters also
/// support independent upstream conformance tests without any network calls.
pub fn action_hash(
    packed_action: &[u8],
    nonce: u64,
    vault: Option<[u8; 20]>,
    expires: Option<u64>,
) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(packed_action.len() + 38);
    bytes.extend_from_slice(packed_action);
    bytes.extend(nonce.to_be_bytes());
    match vault {
        Some(address) => {
            bytes.push(1);
            bytes.extend(address);
        }
        None => bytes.push(0),
    }
    if let Some(expiry) = expires {
        bytes.push(0);
        bytes.extend(expiry.to_be_bytes());
    }
    keccak256(bytes)
}

/// EIP-712 domain ABI words: Exchange, version 1, chain 1337, zero contract.
pub fn domain_separator() -> [u8; 32] {
    let mut words = Vec::with_capacity(160);
    words.extend(keccak256(
        b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
    ));
    words.extend(keccak256(b"Exchange"));
    words.extend(keccak256(b"1"));
    let mut chain = [0u8; 32];
    chain[24..].copy_from_slice(&1337u64.to_be_bytes());
    words.extend(chain);
    words.extend([0u8; 32]);
    keccak256(words)
}

pub fn agent_digest(connection: [u8; 32], mainnet: bool) -> [u8; 32] {
    let mut message = Vec::with_capacity(96);
    message.extend(keccak256(b"Agent(string source,bytes32 connectionId)"));
    message.extend(keccak256(if mainnet { b"a" } else { b"b" }));
    message.extend(connection);
    let mut typed = Vec::with_capacity(66);
    typed.extend([0x19, 0x01]);
    typed.extend(domain_separator());
    typed.extend(keccak256(message));
    keccak256(typed)
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct WireSignature {
    pub r: String,
    pub s: String,
    pub v: u8,
}

pub fn sign_digest(key: &str, digest: [u8; 32]) -> Result<WireSignature> {
    let signature = RecoverableSignature::sign_ecdsa_recoverable(
        Message::from_digest(digest),
        &parse_key(key)?,
    );
    let (recovery, bytes) = signature.serialize_compact();
    let recovery = u8::from(recovery);
    ensure!(recovery <= 1, "unsupported recovery identifier");
    Ok(WireSignature {
        r: format!("0x{}", hex::encode(&bytes[..32])),
        s: format!("0x{}", hex::encode(&bytes[32..])),
        v: 27 + recovery,
    })
}

// Struct declaration order is protocol-significant for named MessagePack maps.
#[derive(Clone, Serialize)]
pub struct OrderAction {
    #[serde(rename = "type")]
    action_type: &'static str,
    orders: Vec<WireOrder>,
    grouping: &'static str,
}
#[derive(Clone, Serialize)]
struct WireOrder {
    a: u32,
    b: bool,
    p: String,
    s: String,
    r: bool,
    t: LimitKind,
    c: String,
}
#[derive(Clone, Serialize)]
struct LimitKind {
    limit: LimitOrder,
}
#[derive(Clone, Serialize)]
struct LimitOrder {
    tif: &'static str,
}

fn decimal_wire(value: f64) -> Result<String> {
    ensure!(value.is_finite() && value > 0.0, "invalid order decimal");
    let fixed = format!("{value:.8}");
    let result = fixed.trim_end_matches('0').trim_end_matches('.');
    let reparsed: f64 = result.parse()?;
    ensure!(
        reparsed > 0.0 && (reparsed - value).abs() < 1e-12,
        "order exceeds wire precision"
    );
    Ok(result.into())
}

impl OrderAction {
    pub fn ioc(
        asset: u32,
        buy: bool,
        price: f64,
        size: f64,
        reduce_only: bool,
        cloid: &str,
    ) -> Result<Self> {
        ensure!(
            asset < 10_000,
            "only default validator perpetual assets are supported"
        );
        let id = uuid::Uuid::parse_str(cloid).context("invalid client order UUID")?;
        ensure!(
            id.to_string() == cloid,
            "client order UUID must be canonical lowercase"
        );
        Ok(Self {
            action_type: "order",
            orders: vec![WireOrder {
                a: asset,
                b: buy,
                p: decimal_wire(price)?,
                s: decimal_wire(size)?,
                r: reduce_only,
                t: LimitKind {
                    limit: LimitOrder { tif: "Ioc" },
                },
                c: format!("0x{}", id.simple()),
            }],
            grouping: "na",
        })
    }
    pub(crate) fn fill_bounds(
        &self,
    ) -> Result<(rust_decimal::Decimal, rust_decimal::Decimal, bool)> {
        let order = self.orders.first().context("submitted order missing")?;
        Ok((
            rust_decimal::Decimal::from_str_exact(&order.s)?,
            rust_decimal::Decimal::from_str_exact(&order.p)?,
            order.b,
        ))
    }
    pub fn packed(&self) -> Result<Vec<u8>> {
        Ok(rmp_serde::to_vec_named(self)?)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExchangePayload<'a> {
    action: &'a OrderAction,
    signature: WireSignature,
    nonce: u64,
    vault_address: Option<String>,
}

fn next_nonce() -> Result<u64> {
    let now = u64::try_from(chrono::Utc::now().timestamp_millis())?;
    allocate_nonce(&LAST_NONCE, now)
}
fn allocate_nonce(last: &AtomicU64, now: u64) -> Result<u64> {
    let old = last
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |prior| {
            let candidate = now.max(prior.checked_add(1)?);
            (candidate <= now.checked_add(1000)?).then_some(candidate)
        })
        .map_err(|_| anyhow::anyhow!("signing nonce exceeds clock window"))?;
    Ok(now.max(old + 1))
}

/// Submit exactly once, to the fixed mainnet endpoint, with no body/signature logs.
/// Transport/HTTP/decoding failures stay errors for the durable unresolved gate.
pub async fn submit_ioc(key: &str, action: &OrderAction) -> Result<ExchangeResponseStatus> {
    // The signed boundary owns transport policy. Callers cannot inject a client
    // that follows 307/308 redirects or retries a signed mutation automatically.
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let nonce = next_nonce()?;
    let signature = sign_digest(
        key,
        agent_digest(action_hash(&action.packed()?, nonce, None, None), true),
    )?;
    let mut response = http
        .post(MAINNET_EXCHANGE)
        .json(&ExchangePayload {
            action,
            signature,
            nonce,
            vault_address: None,
        })
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("exchange transport failed; acknowledgment unresolved"))?;
    ensure!(
        response.status().is_success(),
        "exchange HTTP status {}; acknowledgment unresolved",
        response.status()
    );
    if let Some(length) = response.content_length() {
        ensure!(
            length <= MAX_RESPONSE_BYTES as u64,
            "exchange response exceeds byte limit"
        );
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("exchange response interrupted; acknowledgment unresolved"))?
    {
        ensure!(
            body.len()
                .checked_add(chunk.len())
                .is_some_and(|n| n <= MAX_RESPONSE_BYTES),
            "exchange response exceeds byte limit"
        );
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body)
        .map_err(|_| anyhow::anyhow!("invalid exchange response; acknowledgment unresolved"))
}

// Typed response shape matches the official SDK; unknown variants fail closed.
#[derive(Deserialize, Debug, Clone)]
pub struct RestingOrder {
    pub oid: u64,
}
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FilledOrder {
    pub total_sz: String,
    pub avg_px: String,
    pub oid: u64,
}
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum ExchangeDataStatus {
    Success,
    WaitingForFill,
    WaitingForTrigger,
    Error(String),
    Resting(RestingOrder),
    Filled(FilledOrder),
}
#[derive(Deserialize, Debug, Clone)]
pub struct ExchangeDataStatuses {
    pub statuses: Vec<ExchangeDataStatus>,
}
#[derive(Deserialize, Debug, Clone)]
pub struct ExchangeResponse {
    #[serde(rename = "type")]
    pub response_type: String,
    pub data: Option<ExchangeDataStatuses>,
}
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase", tag = "status", content = "response")]
pub enum ExchangeResponseStatus {
    Ok(ExchangeResponse),
    Err(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nonce_allocation_is_monotonic_and_refuses_clock_drift() {
        let last = AtomicU64::new(0);
        assert_eq!(allocate_nonce(&last, 100).unwrap(), 100);
        assert_eq!(allocate_nonce(&last, 100).unwrap(), 101);
        assert_eq!(allocate_nonce(&last, 90).unwrap(), 102);
        assert!(allocate_nonce(&last, 0).is_ok());
        assert!(allocate_nonce(&last, u64::MAX).is_err());
        let ahead = AtomicU64::new(1100);
        assert!(allocate_nonce(&ahead, 99).is_err());
        assert_eq!(ahead.load(Ordering::SeqCst), 1100);
    }
    #[test]
    fn production_action_uses_canonical_cloid_exact_decimal_and_ordered_msgpack() {
        let action = OrderAction::ioc(
            4,
            true,
            1670.1,
            0.0147,
            false,
            "00000000-0000-0000-0000-000000000001",
        )
        .unwrap();
        let value = serde_json::to_value(&action).unwrap();
        assert_eq!(
            value["orders"][0]["c"],
            "0x00000000000000000000000000000001"
        );
        assert_eq!(value["orders"][0]["p"], "1670.1");
        assert_eq!(value["orders"][0]["t"]["limit"]["tif"], "Ioc");
        let packed = action.packed().unwrap();
        assert_eq!(packed[0], 0x83);
        assert_eq!(&packed[1..6], &[0xa4, b't', b'y', b'p', b'e']);
        assert!(OrderAction::ioc(
            10_000,
            true,
            100.0,
            1.0,
            false,
            "00000000-0000-0000-0000-000000000001"
        )
        .is_err());
        assert!(OrderAction::ioc(
            4,
            true,
            100.0,
            1e-10,
            false,
            "00000000-0000-0000-0000-000000000001"
        )
        .is_err());
        assert!(OrderAction::ioc(
            4,
            true,
            f64::NAN,
            1.0,
            false,
            "00000000-0000-0000-0000-000000000001"
        )
        .is_err());
    }
    #[test]
    fn public_address_matches_known_secp256k1_generator_fixture() {
        let public_fixture = format!("{:064x}", 1);
        assert_eq!(
            derive_address(&public_fixture).unwrap(),
            "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf"
        );
        assert_eq!(
            derive_address(&format!("0x{public_fixture}")).unwrap(),
            derive_address(&public_fixture).unwrap()
        );
    }
    #[test]
    fn invalid_key_errors_never_include_input() {
        let invalid = "PRIVATE_INPUT_FIXTURE";
        assert!(!derive_address(invalid)
            .unwrap_err()
            .to_string()
            .contains(invalid));
        let zero = "0".repeat(64);
        assert!(!sign_digest(&zero, [0; 32])
            .unwrap_err()
            .to_string()
            .contains(&zero));
    }
}
