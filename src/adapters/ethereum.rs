//! Ethereum venue adapter with multi-RPC failover + Uniswap V3 swap execution.

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use ethers::prelude::*;
use ethers::providers::{Http, Provider};
use ethers::signers::{LocalWallet, Signer};
use std::str::FromStr;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::core::{MarketSnapshot, OrderIntent, OrderResult, VenueKind};
use crate::utils::rpc_pool::{ClientHttp, Endpoint, RpcPool};
use super::{default_eth_pair, VenueAdapter, VenuePosition};

const SWAP_ROUTER_02: &str = "0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45";
const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const USDC: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
const WETH_USDC_POOL_05: &str = "0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640";

abigen!(IUniswapV3Pool, r#"[
    function slot0() external view returns (uint160 sqrtPriceX96, int24 tick, uint16 observationIndex, uint16 observationCardinality, uint16 observationCardinalityNext, uint8 feeProtocol, bool unlocked)
    function liquidity() external view returns (uint128)
]"#);
abigen!(IERC20, r#"[
    function balanceOf(address) external view returns (uint256)
    function allowance(address,address) external view returns (uint256)
    function approve(address,uint256) external returns (bool)
    function decimals() external view returns (uint8)
]"#);
abigen!(ISwapRouter02, r#"[
    struct ExactInputSingleParams { address tokenIn; address tokenOut; uint24 fee; address recipient; uint256 amountIn; uint256 amountOutMinimum; uint160 sqrtPriceLimitX96; }
    function exactInputSingle(ExactInputSingleParams params) external payable returns (uint256 amountOut)
]"#);

pub struct EthereumAdapter {
    pool: Arc<RpcPool>,
    chain_id: u64,
    wallet_address: Option<Address>,
    http: reqwest::Client,
}

impl EthereumAdapter {
    pub async fn new(cfg: &AppConfig) -> Result<Self> {
        if cfg.eth_endpoints.is_empty() {
            bail!("ethereum adapter: no RPC endpoints configured");
        }
        let wallet = match cfg.eth_private_key.as_ref() {
            Some(k) => Some(
                LocalWallet::from_str(k.trim_start_matches("0x"))
                    .context("parse ETH_PRIVATE_KEY")?
                    .with_chain_id(cfg.eth_chain_id)
            ),
            None => None,
        };
        let wallet_address = wallet.as_ref().map(|w| w.address());

        let mut endpoints = Vec::with_capacity(cfg.eth_endpoints.len());
        for (name, url) in &cfg.eth_endpoints {
            let provider = match Provider::<Http>::try_from(url.as_str()) {
                Ok(p) => Arc::new(p),
                Err(e) => { tracing::warn!(rpc=%name, error=%e, "skipping unreachable rpc url"); continue; }
            };
            let signer = wallet.as_ref().map(|w| {
                Arc::new(SignerMiddleware::new((*provider).clone(), w.clone()))
            });
            endpoints.push(Endpoint {
                name: name.clone(),
                url: url.clone(),
                provider,
                signer,
            });
        }
        if endpoints.is_empty() { bail!("all configured Ethereum RPC URLs failed to parse"); }

        tracing::info!(
            endpoints = endpoints.len(),
            chain_id = cfg.eth_chain_id,
            addr = ?wallet_address,
            "ethereum adapter init (multi-rpc pool)"
        );

        Ok(Self {
            pool: Arc::new(RpcPool::new(endpoints)),
            chain_id: cfg.eth_chain_id,
            wallet_address,
            http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build()?,
        })
    }

    pub fn pool(&self) -> Arc<RpcPool> { self.pool.clone() }

    async fn uniswap_mid(&self) -> Result<f64> {
        self.pool.run("uniswap_mid", |ep| async move {
            let pool_addr: Address = WETH_USDC_POOL_05.parse()?;
            let pool = IUniswapV3Pool::new(pool_addr, ep.provider.clone());
            let slot0 = pool.slot_0().call().await?;
            let sqrt_price_x96 = slot0.0;
            let num_f = u256_to_f64(U256::from(sqrt_price_x96));
            let ratio = num_f / (2f64.powi(96));
            let raw = ratio * ratio;
            Ok(raw * 10f64.powi(12))
        }).await
    }

    async fn erc20_balance(&self, token: Address, who: Address) -> Result<U256> {
        self.pool.run("erc20_balance", |ep| {
            let c = IERC20::new(token, ep.provider.clone());
            async move { Ok(c.balance_of(who).call().await?) }
        }).await
    }

    async fn recent_candles(&self, lookback: usize) -> Result<Vec<crate::core::Candle>> {
        let now_ms = Utc::now().timestamp_millis();
        let start_ms = now_ms - (lookback as i64) * 60_000;
        let body = serde_json::json!({
            "type":"candleSnapshot",
            "req":{"coin":"ETH","interval":"1m","startTime":start_ms,"endTime":now_ms}
        });
        let raw: Vec<serde_json::Value> = self.http.post("https://api.hyperliquid.xyz/info")
            .json(&body).send().await?.json().await.unwrap_or_default();
        let mut out = Vec::new();
        for c in raw {
            let t = c.get("t").and_then(|v| v.as_i64());
            let o = c.get("o").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
            let h = c.get("h").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
            let l = c.get("l").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
            let c_ = c.get("c").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
            let v = c.get("v").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            if let (Some(t), Some(o), Some(h), Some(l), Some(c_)) = (t, o, h, l, c_) {
                if let Some(ts) = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(t) {
                    out.push(crate::core::Candle { ts, open:o, high:h, low:l, close:c_, volume:v });
                }
            }
        }
        Ok(out)
    }

    async fn first_signer(&self) -> Result<Arc<ClientHttp>> {
        self.pool.endpoints().iter()
            .find_map(|e| e.signer.clone())
            .ok_or_else(|| anyhow!("ETH_PRIVATE_KEY not set"))
    }
}

fn u256_to_f64(v: U256) -> f64 {
    let mut bytes = [0u8; 32]; v.to_big_endian(&mut bytes);
    let mut acc = 0f64;
    for b in bytes { acc = acc * 256.0 + b as f64; }
    acc
}

#[async_trait]
impl VenueAdapter for EthereumAdapter {
    fn kind(&self) -> VenueKind { VenueKind::Ethereum }

    async fn ping(&self) -> Result<()> {
        let id: U256 = self.pool.run("chainId", |ep| async move {
            Ok(ep.provider.get_chainid().await?)
        }).await?;
        if id.as_u64() != self.chain_id {
            tracing::warn!(expected=self.chain_id, got=id.as_u64(), "chainId mismatch");
        }
        Ok(())
    }

    async fn snapshot(&self, symbol: &str, lookback: usize) -> Result<MarketSnapshot> {
        let (_ti, _to, _fee) = default_eth_pair(symbol)?;
        let mid = self.uniswap_mid().await?;
        let candles = self.recent_candles(lookback).await.unwrap_or_default();
        Ok(MarketSnapshot {
            venue: "ethereum".into(),
            symbol: symbol.to_string(),
            mid, bid: None, ask: None, spread_bps: None,
            last_update: Utc::now(),
            candles,
        })
    }

    async fn open_positions(&self) -> Result<Vec<VenuePosition>> {
        let who = match self.wallet_address { Some(a) => a, None => return Ok(vec![]) };
        let bal = self.erc20_balance(WETH.parse()?, who).await.unwrap_or(U256::zero());
        if bal.is_zero() { return Ok(vec![]); }
        let mid = self.uniswap_mid().await.unwrap_or(0.0);
        let qty = u256_to_f64(bal) / 1e18;
        Ok(vec![VenuePosition {
            symbol: "WETH/USDC".into(),
            size: qty,
            entry_price: mid,
            unrealized_pnl: 0.0,
            leverage: 1.0,
        }])
    }

    async fn submit_order(&self, intent: &OrderIntent) -> Result<OrderResult> {
        let who = self.wallet_address.ok_or_else(|| anyhow!("ETH_PRIVATE_KEY required for swap"))?;
        let signer = self.first_signer().await?;
        let (ti, to, fee) = default_eth_pair(&intent.symbol)?;
        let (token_in_addr, token_out_addr, decimals_in) = match (intent.side.is_buy(), ti, to) {
            (true,  "WETH", "USDC") => (USDC, WETH, 6u32),
            (false, "WETH", "USDC") => (WETH, USDC, 18u32),
            (true,  "USDC", "WETH") => (WETH, USDC, 18u32),
            (false, "USDC", "WETH") => (USDC, WETH, 6u32),
            _ => bail!("unsupported intent pair {}/{}", ti, to),
        };
        let token_in: Address  = token_in_addr.parse()?;
        let token_out: Address = token_out_addr.parse()?;
        let router: Address    = SWAP_ROUTER_02.parse()?;

        let mid = self.uniswap_mid().await.unwrap_or(0.0);
        let (amount_in_f, amount_out_min_f) = if intent.side.is_buy() {
            (intent.size * mid * 1.002, intent.size * 0.995)
        } else {
            (intent.size, intent.size * mid * 0.995)
        };

        let amount_in = f64_to_u256_scaled(amount_in_f, decimals_in)?;
        let amount_out_min = f64_to_u256_scaled(
            amount_out_min_f,
            if token_out == USDC.parse::<Address>()? { 6 } else { 18 },
        )?;

        // Approve router if needed (read-only path through pool for allowance)
        let cur_allow: U256 = self.pool.run("allowance", |ep| async move {
            let erc20_in_read = IERC20::new(token_in, ep.provider.clone());
            Ok(erc20_in_read.allowance(who, router).call().await?)
        }).await.unwrap_or(U256::zero());

        if cur_allow < amount_in {
            tracing::info!(token=%format!("{:?}", token_in), "approving SwapRouter02 with max allowance");
            let erc20_in = IERC20::new(token_in, signer.clone());
            let call = erc20_in.approve(router, U256::MAX);
            let pending = call.send().await?;
            let _ = pending.confirmations(1).await?;
        }

        let router_c = ISwapRouter02::new(router, signer.clone());
        let params = ExactInputSingleParams {
            token_in, token_out, fee, recipient: who,
            amount_in, amount_out_minimum: amount_out_min,
            sqrt_price_limit_x96: U256::zero(),
        };
        let tx = router_c.exact_input_single(params);
        let pending = tx.send().await.map_err(|e| anyhow!("swap send: {e}"))?;
        let receipt = pending.confirmations(1).await.map_err(|e| anyhow!("swap confirm: {e}"))?;
        let tx_hash = receipt.as_ref().map(|r| format!("{:?}", r.transaction_hash));

        Ok(OrderResult {
            intent_id: intent.id.clone(),
            accepted: true,
            venue_order_id: tx_hash,
            filled_size: intent.size,
            avg_fill_price: Some(mid),
            message: "uniswap swap confirmed".into(),
        })
    }

    async fn close_position(&self, symbol: &str, size: f64, side_is_buy_to_close: bool) -> Result<OrderResult> {
        let intent = OrderIntent {
            id: uuid::Uuid::new_v4().to_string(),
            venue: "ethereum".into(),
            symbol: symbol.to_string(),
            side: if side_is_buy_to_close { crate::core::Side::Long } else { crate::core::Side::Short },
            size: size.abs(),
            limit_price: None, reduce_only: true,
            stop_price: None, take_profit: None, source_signal: None,
            created_at: Utc::now(),
        };
        self.submit_order(&intent).await
    }

    async fn account_equity_usd(&self) -> Result<f64> {
        let who = match self.wallet_address { Some(a) => a, None => return Ok(0.0) };
        let eth_bal: U256 = self.pool.run("getBalance", |ep| async move {
            Ok(ep.provider.get_balance(who, None).await?)
        }).await.unwrap_or(U256::zero());
        let mid = self.uniswap_mid().await.unwrap_or(0.0);
        let eth  = u256_to_f64(eth_bal) / 1e18;
        let weth = u256_to_f64(self.erc20_balance(WETH.parse()?, who).await.unwrap_or(U256::zero())) / 1e18;
        let usdc = u256_to_f64(self.erc20_balance(USDC.parse()?, who).await.unwrap_or(U256::zero())) / 1e6;
        Ok(eth * mid + weth * mid + usdc)
    }
}

fn f64_to_u256_scaled(v: f64, decimals: u32) -> Result<U256> {
    if v < 0.0 || !v.is_finite() { bail!("invalid scale input {v}"); }
    let scale = 10f64.powi(decimals as i32);
    let scaled = (v * scale).round();
    if scaled > 1e38 { bail!("amount overflows u256 safe range"); }
    Ok(U256::from(scaled as u128))
}
