//! Narrow Hyperliquid order signing boundary. No transfer/withdrawal API exists.
//!
//! MessagePack order and EIP-712 Agent encoding follow the official Python SDK
//! 2fdb18f9517675ea03695a0962bd19eece9c83f0. Cryptographic operations are provided
//! by sha3 and maintained libsecp256k1 bindings, never implemented here.
use anyhow::{ensure, Context, Result};
use secp256k1::{ecdsa::RecoverableSignature, Message, SecretKey};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::str::FromStr;

const MAINNET_EXCHANGE: &str = "https://api.hyperliquid.xyz/exchange";
pub const TESTNET_EXCHANGE: &str = "https://api.hyperliquid-testnet.xyz/exchange";
pub const TESTNET_INFO: &str = "https://api.hyperliquid-testnet.xyz/info";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedAction {
    pub asset: u32,
    pub buy: bool,
    pub price: String,
    pub size: String,
    pub reduce_only: bool,
    pub client_order_id: String,
    pub tif: String,
    pub grouping: String,
    pub messagepack_hex: String,
}

impl UnsignedAction {
    /// Validate retained bytes against their displayed fields without signing.
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.asset < 10_000 && self.tif == "Ioc" && self.grouping == "na",
            "unsupported archived action"
        );
        for value in [&self.price, &self.size] {
            ensure!(value.len() <= 128, "archived decimal exceeds budget");
            let parsed = rust_decimal::Decimal::from_str_exact(value)?;
            ensure!(
                parsed > rust_decimal::Decimal::ZERO
                    && parsed.normalize().scale() <= 8
                    && parsed.normalize().to_string() == *value,
                "noncanonical archived decimal"
            );
        }
        ensure!(
            self.client_order_id.len() == 34 && self.messagepack_hex.len() <= 2048,
            "archived action exceeds budget"
        );
        let id = uuid::Uuid::parse_str(
            self.client_order_id
                .strip_prefix("0x")
                .context("invalid archived client ID")?,
        )?;
        ensure!(
            format!("0x{}", id.simple()) == self.client_order_id,
            "noncanonical archived client ID"
        );
        let action = OrderAction {
            action_type: "order",
            grouping: "na",
            orders: vec![WireOrder {
                a: self.asset,
                b: self.buy,
                p: self.price.clone(),
                s: self.size.clone(),
                r: self.reduce_only,
                t: LimitKind {
                    limit: LimitOrder { tif: "Ioc" },
                },
                c: self.client_order_id.clone(),
            }],
        };
        ensure!(
            hex::encode(action.packed()?) == self.messagepack_hex,
            "archived MessagePack disagrees with action"
        );
        Ok(())
    }
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
    pub(crate) fn unsigned_provenance(&self) -> Result<UnsignedAction> {
        ensure!(
            self.action_type == "order" && self.grouping == "na" && self.orders.len() == 1,
            "unsupported native action provenance"
        );
        let order = &self.orders[0];
        ensure!(
            order.a < 10000 && order.t.limit.tif == "Ioc",
            "unsupported native order provenance"
        );
        Ok(UnsignedAction {
            asset: order.a,
            buy: order.b,
            price: order.p.clone(),
            size: order.s.clone(),
            reduce_only: order.r,
            client_order_id: order.c.clone(),
            tif: order.t.limit.tif.into(),
            grouping: self.grouping.into(),
            messagepack_hex: hex::encode(self.packed()?),
        })
    }
    pub fn packed(&self) -> Result<Vec<u8>> {
        Ok(rmp_serde::to_vec_named(self)?)
    }
}

/// Native Hyperliquid exchange trigger stop-loss wire action (`r: true`, `tpsl: "sl"`).
#[derive(Clone, Serialize)]
pub struct TriggerOrderAction {
    #[serde(rename = "type")]
    action_type: &'static str,
    orders: Vec<TriggerWireOrder>,
    grouping: &'static str,
}

#[derive(Clone, Serialize)]
struct TriggerWireOrder {
    a: u32,
    b: bool,
    p: String,
    s: String,
    r: bool,
    t: TriggerKind,
    c: String,
}

#[derive(Clone, Serialize)]
struct TriggerKind {
    trigger: TriggerSpecWire,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TriggerSpecWire {
    is_market: bool,
    trigger_px: String,
    tpsl: &'static str,
}

impl TriggerOrderAction {
    /// Builds a strict `reduce_only: true` native Hyperliquid stop-loss trigger order.
    pub fn reduce_only_stop(
        asset: u32,
        buy_to_close: bool,
        trigger_price: f64,
        exec_limit_price: f64,
        size: f64,
        cloid: &str,
    ) -> Result<Self> {
        ensure!(asset < 10_000, "unsupported asset id");
        let norm_cloid = if let Ok(id) = uuid::Uuid::parse_str(cloid) {
            format!("0x{}", id.simple())
        } else {
            let trimmed = cloid.trim();
            if trimmed.starts_with("0x")
                && trimmed.len() == 34
                && trimmed[2..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                trimmed.to_ascii_lowercase()
            } else {
                format!("0x{}", hex::encode(&keccak256(trimmed.as_bytes())[..16]))
            }
        };
        Ok(Self {
            action_type: "order",
            orders: vec![TriggerWireOrder {
                a: asset,
                b: buy_to_close,
                p: decimal_wire(exec_limit_price)?,
                s: decimal_wire(size)?,
                r: true,
                t: TriggerKind {
                    trigger: TriggerSpecWire {
                        is_market: true,
                        trigger_px: decimal_wire(trigger_price)?,
                        tpsl: "sl",
                    },
                },
                c: norm_cloid,
            }],
            grouping: "na",
        })
    }

    pub fn packed(&self) -> Result<Vec<u8>> {
        Ok(rmp_serde::to_vec_named(self)?)
    }

    pub fn is_reduce_only(&self) -> bool {
        self.orders.iter().all(|o| o.r && o.t.trigger.tpsl == "sl")
    }

    pub fn client_order_id(&self) -> Option<&str> {
        self.orders.first().map(|o| o.c.as_str())
    }
}

/// Native Hyperliquid exchange cancel-by-oid wire action.
#[derive(Clone, Serialize)]
pub struct CancelAction {
    #[serde(rename = "type")]
    action_type: &'static str,
    cancels: Vec<CancelSpec>,
}

#[derive(Clone, Serialize)]
struct CancelSpec {
    a: u32,
    o: u64,
}

impl CancelAction {
    pub fn by_oid(asset: u32, oid: u64) -> Result<Self> {
        ensure!(asset < 10_000, "unsupported asset id");
        ensure!(oid > 0, "cancel oid must be positive");
        Ok(Self {
            action_type: "cancel",
            cancels: vec![CancelSpec { a: asset, o: oid }],
        })
    }

    pub fn packed(&self) -> Result<Vec<u8>> {
        Ok(rmp_serde::to_vec_named(self)?)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenericExchangePayload<'a, T: Serialize> {
    action: &'a T,
    signature: WireSignature,
    nonce: u64,
    vault_address: Option<String>,
}

async fn post_signed_action<T: Serialize>(
    key: &str,
    packed: &[u8],
    action: &T,
    mainnet: bool,
) -> Result<ExchangeResponseStatus> {
    let signer = derive_address(key)?;
    let nonce = reserve_nonce_at(
        crate::signer_nonce::NonceCoordinator::operator_default()?,
        &signer,
    )
    .await?;
    let endpoint = if mainnet {
        MAINNET_EXCHANGE
    } else {
        TESTNET_EXCHANGE
    };
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let signature = sign_digest(
        key,
        agent_digest(action_hash(packed, nonce, None, None), mainnet),
    )?;
    let mut response = http
        .post(endpoint)
        .json(&GenericExchangePayload {
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

pub async fn submit_trigger_stop(
    key: &str,
    action: &TriggerOrderAction,
    mainnet: bool,
) -> Result<ExchangeResponseStatus> {
    ensure!(
        action.is_reduce_only(),
        "protective stop MUST be reduce_only"
    );
    post_signed_action(key, &action.packed()?, action, mainnet).await
}

pub async fn submit_cancel_by_oid(
    key: &str,
    action: &CancelAction,
    mainnet: bool,
) -> Result<ExchangeResponseStatus> {
    post_signed_action(key, &action.packed()?, action, mainnet).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExchangePayload<'a> {
    action: &'a OrderAction,
    signature: WireSignature,
    nonce: u64,
    vault_address: Option<String>,
}

pub(crate) async fn reserve_nonce_at(
    coordinator: crate::signer_nonce::NonceCoordinator,
    signer: &str,
) -> Result<u64> {
    let signer = signer.to_owned();
    tokio::task::spawn_blocking(move || coordinator.reserve(&signer))
        .await
        .map_err(|_| anyhow::anyhow!("nonce coordinator worker failed"))?
}

/// Submit exactly once, to the fixed mainnet endpoint, with no body/signature logs.
/// Transport/HTTP/decoding failures stay errors for the durable unresolved gate.
pub async fn submit_ioc(key: &str, action: &OrderAction) -> Result<ExchangeResponseStatus> {
    submit_ioc_impl(key, action, None).await
}

#[cfg(not(test))]
pub(crate) async fn submit_ioc_journaled(
    key: &str,
    action: &OrderAction,
    journal: &crate::submission_wire::SubmissionJournal<'_>,
) -> Result<ExchangeResponseStatus> {
    submit_ioc_impl(key, action, Some(journal)).await
}

async fn submit_ioc_impl(
    key: &str,
    action: &OrderAction,
    journal: Option<&crate::submission_wire::SubmissionJournal<'_>>,
) -> Result<ExchangeResponseStatus> {
    let signer = derive_address(key)?;
    let nonce = reserve_nonce_at(
        crate::signer_nonce::NonceCoordinator::operator_default()?,
        &signer,
    )
    .await?;
    if let Some(journal) = journal {
        journal.record_signing_preparation(&signer, nonce, action)?;
    }

    // The signed boundary owns transport policy. Callers cannot inject a client
    // that follows 307/308 redirects or retries a signed mutation automatically.
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
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
    #[tokio::test]
    async fn async_nonce_reservation_shares_durable_history_and_propagates_file_failure() {
        let d = tempfile::tempdir().unwrap();
        let c = crate::signer_nonce::NonceCoordinator::at_root(d.path());
        let signer = "0x1111111111111111111111111111111111111111";
        let first = reserve_nonce_at(c.clone(), signer).await.unwrap();
        let second = reserve_nonce_at(c, signer).await.unwrap();
        assert!(second > first);
        let blocked = d.path().join("not-a-directory");
        std::fs::write(&blocked, b"fixture").unwrap();
        assert!(reserve_nonce_at(
            crate::signer_nonce::NonceCoordinator::at_root(&blocked),
            signer
        )
        .await
        .is_err());
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
        assert_eq!(
            value["orders"][0]["c"],
            crate::order_status::client_order_id("00000000-0000-0000-0000-000000000001").unwrap()
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

    #[test]
    fn native_trigger_stop_enforces_reduce_only_and_cancel_encodes_cleanly() {
        let stop = TriggerOrderAction::reduce_only_stop(
            4,
            false,
            58_800.0,
            58_212.0,
            0.25,
            "protect-BTC-mom-0.25",
        )
        .unwrap();
        assert!(stop.is_reduce_only());
        let val = serde_json::to_value(&stop).unwrap();
        assert_eq!(val["orders"][0]["r"], true);
        assert_eq!(val["orders"][0]["t"]["trigger"]["tpsl"], "sl");
        assert_eq!(val["orders"][0]["t"]["trigger"]["isMarket"], true);
        assert!(!stop.packed().unwrap().is_empty());

        let cancel = CancelAction::by_oid(4, 12345).unwrap();
        let cval = serde_json::to_value(&cancel).unwrap();
        assert_eq!(cval["type"], "cancel");
        assert_eq!(cval["cancels"][0]["o"], 12345);
        assert!(!cancel.packed().unwrap().is_empty());
    }
}
