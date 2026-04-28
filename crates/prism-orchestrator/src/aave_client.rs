//! Aave V3 health-factor query via JSON-RPC eth_call.
//!
//! Calls `Pool.getUserAccountData(address)` (selector 0xbf92857c) and parses
//! the 6×32-byte return: (totalCollateralBase, totalDebtBase, _, _, _,
//! healthFactor). Aave returns USD with 8-decimal scaling; we divide by 1e8
//! to get whole-dollar `HealthFactor` fields.
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

        // Parse first 32 = totalCollateralBase, second 32 = totalDebtBase.
        // Aave uses 8-decimal scaling for *Base values.
        let collateral_usd = parse_u128_be(&bytes[0..32]) / 100_000_000;
        let debt_usd = parse_u128_be(&bytes[32..64]) / 100_000_000;

        Ok(HealthFactor {
            collateral_usd,
            debt_usd,
        })
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
}
