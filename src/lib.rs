//! Termtrader: production-style terminal live trading framework.
//!
//! Supports Hyperliquid perps and Ethereum (data + Uniswap V3 swaps).
//! All live execution is gated behind `ENABLE_LIVE_TRADING` in `.env`.

pub mod account;
pub mod account_positions;
pub mod adapters;
pub mod aegis;
pub mod cli;
#[cfg(feature = "desktop")]
pub mod cockpit;
pub mod config;
pub mod core;
pub mod execution;
pub mod genesis;
pub mod history;
pub mod learning;
pub mod ledger;
pub mod ledger_hyperliquid;
pub mod local_setup;
pub mod monitoring;
pub mod oracle;
pub mod ownership;
pub mod portfolio;
pub mod position_context;
pub mod reporting;
pub mod research;
#[cfg(feature = "desktop")]
pub mod research_ui;
pub mod risk;
pub mod state;
pub mod storage;
pub mod strategies;
pub mod telemetry;
pub mod tui;
pub mod utils;
