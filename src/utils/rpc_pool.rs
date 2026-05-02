//! Multi-endpoint Ethereum JSON-RPC pool with smart failover + cooldown.
//!
//! Responsibilities:
//! * hold an ordered list of HTTP providers (+ optional signer middleware)
//! * expose `run` closure-runner that tries each endpoint in priority order
//! * failed endpoints get exponential-backoff cooldowns, skipped until cool
//! * success on a non-primary endpoint promotes it to "preferred"
//!
//! Intentionally uses a blocking `RwLock` around a small `Vec<State>`. Calls
//! spend ~milliseconds inside the lock; RPC work happens outside.

use anyhow::{anyhow, Result};
use ethers::prelude::*;
use ethers::providers::{Http, Provider};
use ethers::signers::LocalWallet;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::config::defaults::{RPC_COOLDOWN_BASE_SECS, RPC_COOLDOWN_MAX_SECS};

pub type ClientHttp = SignerMiddleware<Provider<Http>, LocalWallet>;

#[derive(Clone)]
pub struct Endpoint {
    pub name: String,
    pub url: String,
    pub provider: Arc<Provider<Http>>,
    pub signer: Option<Arc<ClientHttp>>,
}

#[derive(Debug, Clone, Copy)]
struct State {
    consecutive_failures: u32,
    cooldown_until: Option<Instant>,
    successes: u64,
}
impl Default for State {
    fn default() -> Self { Self { consecutive_failures: 0, cooldown_until: None, successes: 0 } }
}

pub struct RpcPool {
    endpoints: Vec<Endpoint>,
    states: RwLock<Vec<State>>,
    preferred: AtomicUsize,
}

impl RpcPool {
    pub fn new(endpoints: Vec<Endpoint>) -> Self {
        let n = endpoints.len();
        Self {
            endpoints,
            states: RwLock::new(vec![State::default(); n]),
            preferred: AtomicUsize::new(0),
        }
    }

    pub fn endpoints(&self) -> &[Endpoint] { &self.endpoints }

    pub fn len(&self) -> usize { self.endpoints.len() }

    /// Snapshot of per-endpoint health for `hydra health`.
    pub async fn health_snapshot(&self) -> Vec<(String, u32, Option<Duration>, u64)> {
        let st = self.states.read().await;
        self.endpoints.iter().zip(st.iter()).map(|(ep, s)| {
            let remain = s.cooldown_until.and_then(|t| t.checked_duration_since(Instant::now()));
            (ep.name.clone(), s.consecutive_failures, remain, s.successes)
        }).collect()
    }

    /// Pick the next candidate index, skipping endpoints in cooldown.
    /// Returns the logical sequence (preferred first, then round-robin).
    async fn ordering(&self) -> Vec<usize> {
        let preferred = self.preferred.load(Ordering::Relaxed);
        let n = self.endpoints.len();
        let mut out = Vec::with_capacity(n);
        out.push(preferred.min(n.saturating_sub(1)));
        for i in 0..n {
            let idx = (preferred + 1 + i) % n;
            if idx != preferred { out.push(idx); }
        }
        let now = Instant::now();
        let st = self.states.read().await;
        // Move cool endpoints to the back (not removed — last-resort try is OK).
        let (hot, cold): (Vec<_>, Vec<_>) = out.into_iter().partition(|i| {
            st[*i].cooldown_until.map(|t| t <= now).unwrap_or(true)
        });
        let mut merged = hot; merged.extend(cold); merged
    }

    async fn mark_success(&self, idx: usize) {
        let mut st = self.states.write().await;
        st[idx].consecutive_failures = 0;
        st[idx].cooldown_until = None;
        st[idx].successes += 1;
        self.preferred.store(idx, Ordering::Relaxed);
    }

    async fn mark_failure(&self, idx: usize) {
        let mut st = self.states.write().await;
        st[idx].consecutive_failures = st[idx].consecutive_failures.saturating_add(1);
        // Exponential backoff: base * 2^(failures-1), clamped to MAX.
        let n = st[idx].consecutive_failures.min(8);
        let secs = (RPC_COOLDOWN_BASE_SECS.saturating_mul(1u64 << (n - 1).min(6)))
            .min(RPC_COOLDOWN_MAX_SECS);
        st[idx].cooldown_until = Some(Instant::now() + Duration::from_secs(secs));
    }

    /// Run an async closure against the best available endpoint; on failure,
    /// retry the next one. Returns the first successful result or the last error.
    pub async fn run<T, F, Fut>(&self, op: &str, f: F) -> Result<T>
    where
        F: Fn(Endpoint) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let order = self.ordering().await;
        let mut last_err: Option<anyhow::Error> = None;
        for idx in order {
            let ep = self.endpoints[idx].clone();
            match f(ep.clone()).await {
                Ok(v) => {
                    self.mark_success(idx).await;
                    return Ok(v);
                }
                Err(e) => {
                    tracing::warn!(rpc=%ep.name, op=%op, error=%format!("{:#}", e), "endpoint failed, trying next");
                    self.mark_failure(idx).await;
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("rpc pool exhausted for op={op}")))
    }
}
