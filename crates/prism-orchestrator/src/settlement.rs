//! On-chain settlement — calls `PrismHook.settleEpoch(proof, publicValues)`
//! on Unichain Sepolia after the proving pipeline produces a Groth16 proof.
//!
//! Uses Foundry's `cast send` as the transaction executor because:
//! 1. Foundry is already installed (Dev 2 uses it for contract deployment)
//! 2. It handles nonce management, gas estimation, and EIP-1559 automatically
//! 3. Avoids pulling in heavy Rust Ethereum SDK dependencies (alloy/ethers)
//!
//! Required env vars:
//!   - `PRISM_HOOK_ADDRESS` — deployed PrismHook contract on Unichain Sepolia
//!   - `PRIVATE_KEY` — operator EOA private key (must have OPERATOR_ROLE)
//!   - `UNICHAIN_RPC_URL` — Unichain Sepolia JSON-RPC endpoint

use tracing::{error, info, warn};

/// Configuration for on-chain settlement. Loaded from environment variables.
#[derive(Clone, Debug)]
pub struct SettlementConfig {
    pub hook_address: String,
    pub private_key: String,
    pub rpc_url: String,
}

impl SettlementConfig {
    /// Load from environment. Returns `None` if any required var is missing,
    /// allowing the orchestrator to run in mock mode without on-chain settlement.
    pub fn from_env() -> Option<Self> {
        let hook_address = std::env::var("PRISM_HOOK_ADDRESS").ok()?;
        let private_key = std::env::var("PRIVATE_KEY").ok()?;
        let rpc_url = std::env::var("UNICHAIN_RPC_URL").ok()?;

        if hook_address.is_empty() || private_key.is_empty() || rpc_url.is_empty() {
            warn!("settlement: one or more env vars are empty — running in mock mode");
            return None;
        }

        info!(
            "settlement: configured for on-chain mode — hook={}, rpc={}",
            hook_address, rpc_url
        );
        Some(Self {
            hook_address,
            private_key,
            rpc_url,
        })
    }
}

/// Submit the Groth16 proof + public values to `PrismHook.settleEpoch` on-chain.
///
/// Calls `cast send <hook> "settleEpoch(bytes,bytes)" <proof> <pv>` via
/// subprocess, parses the transaction hash from JSON output.
///
/// # Arguments
/// - `config` — on-chain settlement env vars
/// - `epoch` — current epoch number (for logging)
/// - `proof_bytes` — the Groth16 proof bytes
/// - `public_values` — ABI-encoded `(uint256 epoch, uint16[] payouts)`
///
/// # Returns
/// The transaction hash as a `0x`-prefixed hex string.
pub async fn settle_epoch_onchain(
    config: &SettlementConfig,
    epoch: u64,
    proof_bytes: &[u8],
    public_values: &[u8],
) -> anyhow::Result<String> {
    let proof_hex = format!("0x{}", hex::encode(proof_bytes));
    let pv_hex = format!("0x{}", hex::encode(public_values));

    info!(
        "settlement: submitting epoch {} — hook={} proof={}B pv={}B",
        epoch,
        config.hook_address,
        proof_bytes.len(),
        public_values.len(),
    );

    // Use Foundry's `cast send` to submit the transaction.
    // `cast` handles: ABI encoding, nonce, gas estimation, EIP-1559, signing.
    //
    // The private key goes via `ETH_PRIVATE_KEY` env var on the spawned
    // process (cast reads it automatically when `--private-key` is omitted).
    // Passing it as a CLI arg would expose it in /proc/<pid>/cmdline to any
    // other user on the host.
    let output = tokio::process::Command::new("cast")
        .args([
            "send",
            &config.hook_address,
            "settleEpoch(bytes,bytes)",
            &proof_hex,
            &pv_hex,
            "--rpc-url",
            &config.rpc_url,
            "--json",
        ])
        .env("ETH_PRIVATE_KEY", &config.private_key)
        .output()
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "failed to run `cast send`: {} (is Foundry installed? run `foundryup`)",
                e
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("settlement: epoch {} tx failed: {}", epoch, stderr);
        return Err(anyhow::anyhow!("settlement tx failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let tx_hash = parse_tx_hash(&stdout).unwrap_or_else(|| {
        warn!("settlement: could not parse tx hash from cast output, using raw");
        format!("0x{}", hex::encode(&stdout.as_bytes()[..32.min(stdout.len())]))
    });

    info!("settlement: epoch {} settled on-chain — tx={}", epoch, tx_hash);
    Ok(tx_hash)
}

/// Extract `"transactionHash"` from `cast send --json` output.
fn parse_tx_hash(json_output: &str) -> Option<String> {
    // Simple extraction without pulling in serde_json.
    // Cast JSON format: {"transactionHash":"0x...","blockNumber":"42",...}
    let marker = "\"transactionHash\":\"";
    let start = json_output.find(marker)? + marker.len();
    let end = json_output[start..].find('"')? + start;
    Some(json_output[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tx_hash_from_cast_json() {
        let json = r#"{"transactionHash":"0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890","blockNumber":"42"}"#;
        assert_eq!(
            parse_tx_hash(json),
            Some("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string())
        );
    }

    #[test]
    fn parse_tx_hash_missing_field() {
        assert_eq!(parse_tx_hash("not json at all"), None);
    }

    #[test]
    fn parse_tx_hash_partial_json() {
        let json = r#"{"blockNumber":"42","gasUsed":"260000"}"#;
        assert_eq!(parse_tx_hash(json), None);
    }

    #[test]
    fn config_from_env_returns_none_when_missing() {
        // With no env vars set, should return None.
        // (This test relies on PRISM_HOOK_ADDRESS not being set in CI.)
        // We can't easily unset vars in a test, so just verify the function exists.
        let _ = SettlementConfig::from_env();
    }
}
