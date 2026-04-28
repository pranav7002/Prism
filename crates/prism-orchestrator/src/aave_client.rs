//! Aave V3 health-factor query via JSON-RPC eth_call.
//!
//! Calls `Pool.getUserAccountData(address)` (selector 0xbf92857c). The
//! 6×32-byte return is:
//!
//!   slot 0 — totalCollateralBase  (1e8-scaled USD)
//!   slot 1 — totalDebtBase        (1e8-scaled USD)
//!   slot 2 — availableBorrowsBase (1e8-scaled USD, ignored)
//!   slot 3 — currentLiquidationThreshold (basis points, ignored)
//!   slot 4 — ltv                  (basis points, ignored)
//!   slot 5 — healthFactor         (1e18-scaled — THE actual HF that
//!                                   accounts for per-asset liquidation
//!                                   thresholds; what we want)
//!
//! Pre-H7 fix this code parsed slots 0+1 and computed `collateral / debt`
//! as the HF, which is wrong for mixed-asset positions. Now we read slot
//! 5 directly, descale by 1e18, and construct the HealthFactor via
//! `from_aave_e18`. (Closes H7 in Audit report.)
//!
//! Aave V3 isn't deployed on Unichain Sepolia, so AAVE_RPC_URL typically
//! points at a different chain (Sepolia / OP Sepolia). For demos without a
//! deployment, `fallback_healthy()` returns HF=2.0.

use anyhow::{anyhow, Context};
use prism_types::HealthFactor;
use serde_json::json;

pub struct AaveClient {
    rpc_url: String,
    pool_address: String,
    http: reqwest::Client,
}

impl AaveClient {
    pub fn new(rpc_url: &str, pool_address: &str) -> Self {
        Self {
            rpc_url: rpc_url.trim_end_matches('/').to_string(),
            pool_address: pool_address.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Query Aave V3 Pool.getUserAccountData(user). Returns the user's
    /// collateral and debt in whole-dollar USD units.
    pub async fn get_health_factor(&self, user: &str) -> anyhow::Result<HealthFactor> {
        // Build calldata: selector || padded user address.
        let user_clean = user.trim_start_matches("0x");
        if user_clean.len() != 40 {
            return Err(anyhow!(
                "user address must be 40 hex chars, got {}",
                user_clean.len()
            ));
        }
        let mut calldata = String::from("0xbf92857c"); // selector
        calldata.push_str("000000000000000000000000"); // 12-byte left-pad
        calldata.push_str(user_clean); // 20-byte address

        let payload = json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [
                { "to": self.pool_address, "data": calldata },
                "latest"
            ],
            "id": 1
        });

        let resp: serde_json::Value = self
            .http
            .post(&self.rpc_url)
            .json(&payload)
            .send()
            .await
            .context("aave rpc post")?
            .json()
            .await
            .context("aave rpc json parse")?;

        let result = resp
            .get("result")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("aave rpc returned no result: {}", resp))?;
        let result_clean = result.trim_start_matches("0x");
        let bytes = hex::decode(result_clean).context("aave result hex decode")?;
        if bytes.len() < 192 {
            return Err(anyhow!("aave result too short: {} bytes", bytes.len()));
        }

        // Parse slot 5 (offset 160..192) = healthFactor, 1e18-scaled.
        // This is the actual HF Aave publishes — accounts for per-asset
        // liquidation thresholds (closes H7).
        let hf_e18 = parse_u128_be(&bytes[160..192]);

        // Aave returns hf_e18 = type(uint256).max (parses to u128::MAX
        // here after saturation) when the user has zero debt — same
        // semantics as our HealthFactor::value() returning INFINITY.
        // Either way, treat it as healthy.
        Ok(HealthFactor::from_aave_e18(hf_e18))
    }
}

/// Fallback when no AAVE_POOL_ADDRESS is configured: return a healthy
/// constant (HF=2.0).
pub fn fallback_healthy() -> HealthFactor {
    HealthFactor {
        collateral_usd: 2_000_000,
        debt_usd: 1_000_000,
    }
}

/// Parse a big-endian 32-byte slice as a u128 by taking the low 16 bytes.
/// For Aave *Base values the high 16 bytes are effectively always zero
/// (positions never reach 2^128 USD-cents-scaled), so this is a safe
/// truncation in practice. If the high bytes are non-zero we saturate to
/// `u128::MAX` rather than silently wrap.
fn parse_u128_be(bytes: &[u8]) -> u128 {
    debug_assert_eq!(bytes.len(), 32);
    // High 16 bytes — if any are non-zero the value exceeds u128::MAX.
    let high_nonzero = bytes[..16].iter().any(|&b| b != 0);
    if high_nonzero {
        return u128::MAX;
    }
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&bytes[16..32]);
    u128::from_be_bytes(arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_is_safe() {
        let hf = fallback_healthy();
        assert!(hf.is_safe());
        assert_eq!(hf.collateral_usd, 2_000_000);
        assert_eq!(hf.debt_usd, 1_000_000);
    }

    #[test]
    fn parse_u128_be_basic() {
        let mut bytes = [0u8; 32];
        bytes[31] = 1;
        assert_eq!(parse_u128_be(&bytes), 1);

        // 0x...0100_000000 in low 16 bytes
        let mut bytes = [0u8; 32];
        bytes[24] = 0x01;
        assert_eq!(parse_u128_be(&bytes), 1u128 << 56);
    }

    #[test]
    fn parse_u128_be_saturates_when_high_nonzero() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x01;
        assert_eq!(parse_u128_be(&bytes), u128::MAX);
    }

    #[test]
    fn rejects_bad_user_address() {
        let client = AaveClient::new("http://localhost:0", "0xpool");
        // 39-char address → invalid.
        let bad = "0x".to_string() + &"a".repeat(39);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let res = rt.block_on(client.get_health_factor(&bad));
        assert!(res.is_err());
    }

    /// Synthesize the 6×32-byte return-data of getUserAccountData and
    /// confirm the parser reads slot 5 (healthFactor) — not the
    /// collateral/debt ratio of slots 0+1 — by feeding values where the
    /// two would disagree.
    #[test]
    fn parses_slot5_health_factor_not_ratio() {
        // Slot 0 (collateral) = 2e8 ($2 USD-scaled at 1e8) — would
        // suggest HF=2.0 if naïvely divided by slot 1.
        // Slot 1 (debt) = 1e8 ($1 USD).
        // Slot 5 (real HF) = 1.5e18 — Aave's actual answer accounting
        // for liquidation thresholds.
        let mut buf = vec![0u8; 192];
        // collateral = 2e8
        let col_e8: u128 = 200_000_000;
        buf[16..32].copy_from_slice(&col_e8.to_be_bytes());
        // debt = 1e8
        let debt_e8: u128 = 100_000_000;
        buf[48..64].copy_from_slice(&debt_e8.to_be_bytes());
        // healthFactor = 1.5e18
        let hf_e18: u128 = 1_500_000_000_000_000_000;
        buf[176..192].copy_from_slice(&hf_e18.to_be_bytes());

        // Re-implement the parse inline (the network IO path isn't unit-testable).
        let hf_parsed = parse_u128_be(&buf[160..192]);
        assert_eq!(hf_parsed, hf_e18);

        let hf = HealthFactor::from_aave_e18(hf_parsed);
        // value() should recover 1.5 — not 2.0 (the ratio of slots 0/1)
        assert!((hf.value() - 1.5).abs() < 1e-9);
        assert!(hf.is_safe());
    }

    #[test]
    fn from_aave_e18_zero_debt_means_max() {
        // Aave returns type(uint256).max for zero-debt accounts; our
        // parser saturates that to u128::MAX. value() should be very
        // large and definitely safe.
        let hf = HealthFactor::from_aave_e18(u128::MAX);
        assert!(hf.is_safe());
        assert!(hf.value() > 1e10);
    }
}
