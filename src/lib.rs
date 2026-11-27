//! Termtrader: production-style terminal live trading framework.
//!
//! Supports Hyperliquid perps and Ethereum (data + Uniswap V3 swaps).
//! All live execution is gated behind `ENABLE_LIVE_TRADING` in `.env`.

pub mod account;
pub mod account_positions;
pub mod adapters;
pub mod aegis;
pub mod aegis_sovereign;
pub mod cli;
#[cfg(feature = "desktop")]
pub mod cockpit;
pub mod config;
pub mod core;
pub mod engine_ipc;
pub mod evolution;
#[cfg(feature = "desktop")]
pub mod evolution_ui;
pub mod execution;
pub mod execution_recovery;
pub mod genesis;
pub mod genesis_nsga2;
pub mod history;
pub mod hyperliquid_wire;
pub mod learning;
pub mod ledger;
pub mod ledger_hyperliquid;
pub mod local_setup;
pub mod monitoring;
pub mod oracle;
pub mod oracle_research;
pub mod ownership;
pub mod portfolio;
pub mod position_context;
pub mod protective_orders;
pub mod reporting;
pub mod research;
#[cfg(feature = "desktop")]
pub mod research_ui;
pub mod risk;
pub mod settlement;
pub mod state;
pub mod storage;
pub mod strategies;
pub mod telemetry;
pub mod treasury;
#[cfg(feature = "desktop")]
pub mod treasury_ui;
pub mod tui;
pub mod utils;

pub mod execution_facts;
pub mod hunter;
pub mod hunter_microstructure;
pub mod hunter_stream;
pub mod intent_evidence;
pub mod live_gate;

pub mod order_status;
pub mod paper_pipeline;
pub mod paper_service;
pub mod paper_store;

pub mod status_history;
pub mod synapse_bus;

pub mod signer_nonce;
mod signing_preparation;
pub mod submission_wire;
pub mod wire_history;

#[cfg(feature = "desktop")]
pub mod hunter_ui;

pub mod oracle_validation;

pub mod cognition_shadow;
pub mod cognition_stats;
pub mod fang_engine;
pub mod fang_planner;
pub mod portfolio_research;

pub mod diagnostics;
#[cfg(feature = "desktop")]
pub mod diagnostics_ui;

pub mod genesis_checkpoint;

pub mod genesis_oos;

#[cfg(feature = "desktop")]
pub mod desktop_renderer;

#[cfg(feature = "desktop")]
pub mod leviathan_3d_math;
#[cfg(feature = "desktop")]
pub mod leviathan_awareness;
#[cfg(feature = "desktop")]
pub mod leviathan_layout;
#[cfg(feature = "desktop-wgpu")]
pub mod leviathan_mesh;
#[cfg(feature = "desktop-wgpu")]
pub mod leviathan_renderer_3d;
#[cfg(feature = "desktop")]
pub mod leviathan_scene;
#[cfg(feature = "desktop-wgpu")]
pub mod leviathan_shader;
#[cfg(feature = "desktop")]
pub mod leviathan_spatial;
#[cfg(feature = "desktop")]
pub mod leviathan_state;
#[cfg(feature = "desktop")]
pub mod startup_diagnostics;
