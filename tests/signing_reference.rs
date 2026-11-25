//! Offline cross-language conformance fixtures, not an exchange client.
//!
//! Expected hashes/signatures are published by the official Hyperliquid Python SDK:
//! https://github.com/hyperliquid-dex/hyperliquid-python-sdk/blob/2fdb18f9517675ea03695a0962bd19eece9c83f0/tests/signing_test.py
//! The fixture identity is public upstream test material. Never fund or deploy it.
//! No environment variables, account probes, ExchangeClient or network calls are used.

use hydra::hyperliquid_wire::{self, keccak256, WireSignature as Signature};
type H160 = [u8; 20];

// A deliberately small test encoder. It implements only the MessagePack forms used
// by these published fixtures. This is NOT a production/general-purpose codec.
#[derive(Clone)]
enum Wire {
    Text(String),
    Number(u64),
    Flag(bool),
    List(Vec<Wire>),
    Map(Vec<(&'static str, Wire)>),
}

impl Wire {
    fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }
    fn pack(&self, out: &mut Vec<u8>) {
        match self {
            Self::Text(value) => {
                let bytes = value.as_bytes();
                match bytes.len() {
                    0..=31 => out.push(0xa0 | bytes.len() as u8),
                    32..=255 => out.extend([0xd9, bytes.len() as u8]),
                    _ => panic!("fixture string exceeds the test codec's supported length"),
                }
                out.extend(bytes);
            }
            Self::Number(value) => match value {
                0..=127 => out.push(*value as u8),
                128..=255 => out.extend([0xcc, *value as u8]),
                256..=65535 => {
                    out.push(0xcd);
                    out.extend((*value as u16).to_be_bytes());
                }
                65536..=4294967295 => {
                    out.push(0xce);
                    out.extend((*value as u32).to_be_bytes());
                }
                _ => {
                    out.push(0xcf);
                    out.extend(value.to_be_bytes());
                }
            },
            Self::Flag(value) => out.push(if *value { 0xc3 } else { 0xc2 }),
            Self::List(values) => {
                assert!(values.len() < 16);
                out.push(0x90 | values.len() as u8);
                for value in values {
                    value.pack(out);
                }
            }
            Self::Map(fields) => {
                assert!(fields.len() < 16);
                out.push(0x80 | fields.len() as u8);
                // Field insertion order is part of the signed preimage.
                for (key, value) in fields {
                    Self::text(*key).pack(out);
                    value.pack(out);
                }
            }
        }
    }
}

fn action_hash(action: &Wire, nonce: u64, vault: Option<H160>, expires: Option<u64>) -> [u8; 32] {
    let mut bytes = Vec::new();
    action.pack(&mut bytes);
    bytes.extend(nonce.to_be_bytes());
    match vault {
        None => bytes.push(0),
        Some(address) => {
            bytes.push(1);
            bytes.extend(address);
        }
    }
    if let Some(expires) = expires {
        bytes.push(0);
        bytes.extend(expires.to_be_bytes());
    }
    keccak256(bytes)
}

fn agent_payload(connection: [u8; 32], mainnet: bool) -> [u8; 32] {
    hyperliquid_wire::agent_digest(connection, mainnet)
}

fn public_fixture_key() -> String {
    // Reconstruct PUBLIC upstream fixture; never load any wallet/environment.
    let digits = b"0123456789";
    (0..64).map(|i| digits[i % digits.len()] as char).collect()
}

fn signature(action: &Wire, mainnet: bool, vault: Option<H160>) -> Signature {
    hyperliquid_wire::sign_digest(
        &public_fixture_key(),
        agent_payload(action_hash(action, 0, vault, None), mainnet),
    )
    .unwrap()
}

fn assert_signature(actual: Signature, r: &str, s: &str, v: u64) {
    assert_eq!(
        actual.r.trim_start_matches("0x").trim_start_matches('0'),
        r.trim_start_matches('0')
    );
    assert_eq!(
        actual.s.trim_start_matches("0x").trim_start_matches('0'),
        s.trim_start_matches('0')
    );
    assert_eq!(actual.v as u64, v);
}

fn dummy() -> Wire {
    Wire::Map(vec![
        ("type", Wire::text("dummy")),
        ("num", Wire::Number(100_000_000_000)),
    ])
}

fn order(
    asset: u64,
    price: &str,
    size: &str,
    tif: &str,
    cloid: Option<&str>,
    trigger: bool,
) -> Wire {
    let kind = if trigger {
        Wire::Map(vec![(
            "trigger",
            Wire::Map(vec![
                ("isMarket", Wire::Flag(true)),
                ("triggerPx", Wire::text("103")),
                ("tpsl", Wire::text("sl")),
            ]),
        )])
    } else {
        Wire::Map(vec![("limit", Wire::Map(vec![("tif", Wire::text(tif))]))])
    };
    let mut fields = vec![
        ("a", Wire::Number(asset)),
        ("b", Wire::Flag(true)),
        ("p", Wire::text(price)),
        ("s", Wire::text(size)),
        ("r", Wire::Flag(false)),
        ("t", kind),
    ];
    if let Some(cloid) = cloid {
        fields.push(("c", Wire::text(cloid)));
    }
    Wire::Map(vec![
        ("type", Wire::text("order")),
        ("orders", Wire::List(vec![Wire::Map(fields)])),
        ("grouping", Wire::text("na")),
    ])
}

#[test]
fn published_production_action_hash_matches_ordered_msgpack() {
    let action = order(4, "1670.1", "0.0147", "Ioc", None, false);
    assert_eq!(
        hex::encode(action_hash(&action, 1_677_777_606_040, None, None)),
        "0fcbeda5ae3c4950a548021552a4fea2226858c4453571bf3f24ba017eac2908"
    );
}

#[test]
fn published_dummy_action_signatures_match_both_networks() {
    assert_signature(
        signature(&dummy(), true, None),
        "53749d5b30552aeb2fca34b530185976545bb22d0b3ce6f62e31be961a59298",
        "755c40ba9bf05223521753995abb2f73ab3229be8ec921f350cb447e384d8ed8",
        27,
    );
    assert_signature(
        signature(&dummy(), false, None),
        "542af61ef1f429707e3c76c5293c80d01f74ef853e34b76efffcb57e574f9510",
        "17b8b32f086e8cdede991f1e2c529f5dd5297cbe8128500e00cbaf766204a613",
        28,
    );
}

#[test]
fn published_limit_order_signatures_match_both_networks() {
    let action = order(1, "100", "100", "Gtc", None, false);
    assert_signature(
        signature(&action, true, None),
        "d65369825a9df5d80099e513cce430311d7d26ddf477f5b3a33d2806b100d78e",
        "2b54116ff64054968aa237c20ca9ff68000f977c93289157748a3162b6ea940e",
        28,
    );
    assert_signature(
        signature(&action, false, None),
        "82b2ba28e76b3d761093aaded1b1cdad4960b3af30212b343fb2e6cdfa4e3d54",
        "6b53878fc99d26047f4d7e8c90eb98955a109f44209163f52d8dc4278cbbd9f5",
        27,
    );
}

#[test]
fn published_cloid_order_signatures_match_both_networks() {
    let action = order(
        1,
        "100",
        "100",
        "Gtc",
        Some("0x00000000000000000000000000000001"),
        false,
    );
    assert_signature(
        signature(&action, true, None),
        "41ae18e8239a56cacbc5dad94d45d0b747e5da11ad564077fcac71277a946e3",
        "3c61f667e747404fe7eea8f90ab0e76cc12ce60270438b2058324681a00116da",
        27,
    );
    assert_signature(
        signature(&action, false, None),
        "eba0664bed2676fc4e5a743bf89e5c7501aa6d870bdb9446e122c9466c5cd16d",
        "7f3e74825c9114bc59086f1eebea2928c190fdfbfde144827cb02b85bbe90988",
        28,
    );
}

#[test]
fn published_vault_signatures_match_both_networks() {
    let vault = Some(
        hex::decode("1719884eb866cb12b2287399b15f7db5e7d775ea")
            .unwrap()
            .try_into()
            .unwrap(),
    );
    assert_signature(
        signature(&dummy(), true, vault),
        "3c548db75e479f8012acf3000ca3a6b05606bc2ec0c29c50c515066a326239",
        "4d402be7396ce74fbba3795769cda45aec00dc3125a984f2a9f23177b190da2c",
        28,
    );
    assert_signature(
        signature(&dummy(), false, vault),
        "e281d2fb5c6e25ca01601f878e4d69c965bb598b88fac58e475dd1f5e56c362b",
        "7ddad27e9a238d045c035bc606349d075d5c5cd00a6cd1da23ab5c39d4ef0f60",
        27,
    );
}

#[test]
fn published_trigger_order_signatures_match_both_networks() {
    let action = order(1, "100", "100", "Gtc", None, true);
    assert_signature(
        signature(&action, true, None),
        "98343f2b5ae8e26bb2587daad3863bc70d8792b09af1841b6fdd530a2065a3f9",
        "6b5bb6bb0633b710aa22b721dd9dee6d083646a5f8e581a20b545be6c1feb405",
        27,
    );
    assert_signature(
        signature(&action, false, None),
        "971c554d917c44e0e1b6cc45d8f9404f32172a9d3b3566262347d0302896a2e4",
        "206257b104788f80450f8e786c329daa589aa0b32ba96948201ae556d5637eac",
        28,
    );
}

#[test]
fn production_action_hash_matches_independent_fixture_encoder() {
    let action = hyperliquid_wire::OrderAction::ioc(
        4,
        true,
        1670.1,
        0.0147,
        false,
        "00000000-0000-0000-0000-000000000001",
    )
    .unwrap();
    let expected = order(
        4,
        "1670.1",
        "0.0147",
        "Ioc",
        Some("0x00000000000000000000000000000001"),
        false,
    );
    let mut expected_bytes = Vec::new();
    expected.pack(&mut expected_bytes);
    assert_eq!(action.packed().unwrap(), expected_bytes);
    assert_eq!(
        hyperliquid_wire::action_hash(&action.packed().unwrap(), 1_677_777_606_040, None, None),
        action_hash(&expected, 1_677_777_606_040, None, None)
    );
}

#[test]
fn nonce_vault_expiry_network_and_field_order_are_signature_boundaries() {
    let action = dummy();
    let base = action_hash(&action, 0, None, None);
    assert_ne!(base, action_hash(&action, 1, None, None));
    assert_ne!(base, action_hash(&action, 0, Some([0; 20]), None));
    assert_ne!(base, action_hash(&action, 0, None, Some(0)));
    assert_ne!(
        base,
        action_hash(
            &Wire::Map(vec![
                ("num", Wire::Number(100_000_000_000)),
                ("type", Wire::text("dummy"))
            ]),
            0,
            None,
            None
        )
    );
    assert_ne!(agent_payload(base, true), agent_payload(base, false));
}
