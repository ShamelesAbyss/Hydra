//! Pure account identity and balance contracts, tested without exchange access.
use crate::ownership::canonical_account;
use anyhow::{ensure, Result};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct UserRole {
    pub role: String,
    pub data: Option<RoleData>,
}
#[derive(Deserialize)]
pub struct RoleData {
    pub user: Option<String>,
}

pub fn verify_signer(
    signer: &str,
    account: &str,
    signer_role: &UserRole,
    account_role: &UserRole,
) -> Result<()> {
    let signer = canonical_account(signer)?;
    let account = canonical_account(account)?;
    ensure!(
        account_role.role == "user",
        "vault/subaccount/missing trading identities are not yet supported"
    );
    if signer == account {
        ensure!(
            signer_role.role == "user",
            "master signer role is not a user"
        );
    } else {
        ensure!(
            signer_role.role == "agent",
            "separate signer is not an approved account agent"
        );
        let owner = signer_role
            .data
            .as_ref()
            .and_then(|d| d.user.as_deref())
            .ok_or_else(|| anyhow::anyhow!("agent owner unavailable"))?;
        ensure!(
            canonical_account(owner)? == account,
            "agent belongs to a different trading account"
        );
    }
    Ok(())
}

/// Standard perp equity is already an exchange-valued amount. Spot holdings are
/// not perp collateral in this mode. Other modes need their own authoritative model.
pub fn standard_perp_equity(mode: &str, account_value: &str) -> Result<f64> {
    ensure!(
        mode == "disabled",
        "account mode {mode} requires a separately verified collateral model; sizing unavailable"
    );
    let value: f64 = account_value.parse()?;
    ensure!(
        value.is_finite() && value >= 0.0,
        "invalid perp account equity"
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: &str = "0x1111111111111111111111111111111111111111";
    const B: &str = "0x2222222222222222222222222222222222222222";
    fn role(value: serde_json::Value) -> UserRole {
        serde_json::from_value(value).unwrap()
    }
    #[test]
    fn agent_must_be_bound_to_the_explicit_account() {
        let owner = role(serde_json::json!({"role":"user"}));
        let agent = role(serde_json::json!({"role":"agent","data":{"user":A}}));
        verify_signer(B, A, &agent, &owner).unwrap();
        assert!(verify_signer(A, B, &agent, &owner).is_err());
        assert!(verify_signer(B, A, &owner, &owner).is_err());
        verify_signer(A, A, &owner, &owner).unwrap();
    }
    #[test]
    fn balance_modes_never_guess_or_add_spot_collateral() {
        assert_eq!(standard_perp_equity("disabled", "123.45").unwrap(), 123.45);
        for mode in [
            "unifiedAccount",
            "portfolioMargin",
            "default",
            "dexAbstraction",
            "unknown",
        ] {
            assert!(standard_perp_equity(mode, "99999").is_err());
        }
        for bad in ["NaN", "inf", "-1", "bad"] {
            assert!(standard_perp_equity("disabled", bad).is_err());
        }
    }
}
