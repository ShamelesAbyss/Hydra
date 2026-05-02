//! Termtrader: production-style terminal live trading framework.
//!
//! Supports Hyperliquid perps and Ethereum (data + Uniswap V3 swaps).
//! All live execution is gated behind `ENABLE_LIVE_TRADING` in `.env`.

pub mod adapters;
pub mod cli;
pub mod config;
pub mod core;
pub mod execution;
pub mod learning;
pub mod monitoring;
pub mod portfolio;
pub mod reporting;
pub mod risk;
pub mod state;
pub mod storage;
pub mod strategies;
pub mod tui;
pub mod utils;
