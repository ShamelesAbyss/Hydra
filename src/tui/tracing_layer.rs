//! `tracing` subscriber layer that funnels events into the dashboard feed.
//!
//! Attached only when the TUI is active so we don't corrupt the alternate
//! screen with stdout writes. Errors are also duplicated into the error ring.

use chrono::Utc;
use std::fmt::Write as _;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

use super::state::{ErrorRow, FeedLine, SharedDash};

pub struct DashLayer {
    dash: SharedDash,
}

impl DashLayer {
    pub fn new(dash: SharedDash) -> Self {
        Self { dash }
    }
}

struct MessageVisitor {
    message: String,
    kv: String,
}
impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{:?}", value);
            // strip surrounding quotes if present
            if self.message.starts_with('"')
                && self.message.ends_with('"')
                && self.message.len() >= 2
            {
                self.message = self.message[1..self.message.len() - 1].to_string();
            }
        } else {
            if !self.kv.is_empty() {
                self.kv.push(' ');
            }
            let _ = write!(self.kv, "{}={:?}", field.name(), value);
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            if !self.kv.is_empty() {
                self.kv.push(' ');
            }
            let _ = write!(self.kv, "{}={}", field.name(), value);
        }
    }
}

impl<S: Subscriber> Layer<S> for DashLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let level = meta.level();
        let target = meta.target().to_string();

        let mut v = MessageVisitor {
            message: String::new(),
            kv: String::new(),
        };
        event.record(&mut v);

        // Combine message + key/value pairs so the operator sees context.
        let composed = if v.kv.is_empty() {
            v.message.clone()
        } else {
            format!("{}  {}", v.message, v.kv)
        };

        let level_str = match *level {
            Level::ERROR => "ERROR",
            Level::WARN => "WARN",
            Level::INFO => "INFO",
            Level::DEBUG => "DEBUG",
            Level::TRACE => "TRACE",
        };

        // Never let a lock contention on the dashboard state affect trading.
        if let Ok(mut d) = self.dash.try_write() {
            d.push_feed(FeedLine {
                ts: Utc::now(),
                level: level_str.to_string(),
                target: target.clone(),
                message: composed.clone(),
            });
            // Duplicate ERROR/WARN into the dedicated errors pane.
            if matches!(*level, Level::ERROR | Level::WARN) {
                d.push_error(ErrorRow {
                    ts: Utc::now(),
                    kind: target,
                    message: composed,
                });
            }
        }
    }
}
