//! Shared startup and pre-entry validation of net-position ownership.
use crate::{adapters::VenuePosition, core::OpenPosition};
use anyhow::{ensure, Result};

pub fn validate(local: &[OpenPosition], remote: &[VenuePosition], venue: &str) -> Result<()> {
    let local: Vec<_> = local
        .iter()
        .filter(|position| position.venue == venue)
        .collect();
    for position in &local {
        ensure!(
            position.size.is_finite()
                && position.size > 0.0
                && position.entry_price.is_finite()
                && position.entry_price > 0.0
                && matches!(position.side.as_str(), "LONG" | "SHORT"),
            "invalid persisted net position"
        );
        ensure!(
            remote.iter().any(|other| other.symbol == position.symbol),
            "local position {} missing remotely; fill reconciliation required",
            position.symbol
        );
    }
    for position in remote {
        ensure!(
            position.size.is_finite()
                && position.size != 0.0
                && position.entry_price.is_finite()
                && position.entry_price > 0.0,
            "invalid exchange net position"
        );
        ensure!(
            remote
                .iter()
                .filter(|other| other.symbol == position.symbol)
                .count()
                == 1,
            "duplicate exchange net position"
        );
        let matches: Vec<_> = local
            .iter()
            .filter(|other| other.symbol == position.symbol)
            .collect();
        ensure!(
            matches.len() == 1,
            "unknown or duplicate position {}; reconciliation unresolved",
            position.symbol
        );
        let owned = matches[0];
        let signed = owned.size * if owned.side == "LONG" { 1.0 } else { -1.0 };
        ensure!(
            (signed - position.size).abs() <= 1e-10
                && (owned.entry_price - position.entry_price).abs() <= 1e-8,
            "position mismatch {}; reconciliation unresolved",
            position.symbol
        );
    }
    Ok(())
}
