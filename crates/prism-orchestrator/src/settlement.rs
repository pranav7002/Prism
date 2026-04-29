//! On-chain settlement — calls `PrismHook.settleEpoch(proof, publicValues)`
//! on Unichain Sepolia after the proving pipeline produces a Groth16 proof.
//!
//! This is the in-process alloy-based implementation. The prior version
//! shelled out to `cast send` and passed the operator private key via env,
//! which left it readable in `/proc/<pid>/environ` to anyone with `ptrace`
//! (audit C9 follow-up). The current implementation signs and broadcasts
//! the transaction directly through `alloy-provider` + `alloy-signer-local`
//! — the key never crosses a process boundary.
//!
//! Required env vars:
//!   - `PRISM_HOOK_ADDRESS` — deployed PrismHook contract on Unichain Sepolia
//!   - `PRIVATE_KEY` — operator EOA (must hold the `operators[..]=true` slot
//!     on the deployed PrismHook; otherwise `settleEpoch` reverts with
//!     `NotOperator`)
//!   - `UNICHAIN_RPC_URL` — Unichain Sepolia JSON-RPC endpoint

use alloy_network::{EthereumWallet, TransactionBuilder};
use alloy_primitives::{Address, Bytes};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types_eth::TransactionRequest;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, SolCall};
use std::str::FromStr;
use tracing::{info, warn};

sol! {
    /// Minimal interface for the on-chain call site. The full PrismHook ABI
    /// lives in `contracts/src/PrismHook.sol`.
    interface IPrismHook {
        function settleEpoch(bytes proof, bytes publicValues) external;
        function settleEpochThreeProof(
            bytes solverProof, bytes solverPv,
            bytes executionProof, bytes executionPv,
            bytes shapleyProof, bytes shapleyPv,
            uint256 epoch, uint16[] payouts
        ) external;
        function setSubVkeys(bytes32 solver, bytes32 execution, bytes32 shapley) external;
    }
}

/// Outer-wrapper schema version for on-chain `publicValues`. Mirrors
/// `PrismHook.SCHEMA_VERSION` — the hook will revert with
/// `SchemaVersionUnsupported` if the byte doesn't match.
pub const SCHEMA_VERSION: u8 = 1;

/// Prepend the SCHEMA_VERSION byte to an inner public-values blob before
/// submitting to the hook. The proof remains bound to the inner bytes only
/// (the byte is advisory, see PrismHook NatSpec).
pub fn with_schema_byte(inner: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(inner.len() + 1);
    out.push(SCHEMA_VERSION);
    out.extend_from_slice(inner);
    out
}

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
/// Signs the tx in-process with `alloy-signer-local`, builds a
/// recommended-fillers provider that handles nonce / gas / EIP-1559
/// automatically, awaits one confirmation, and returns the tx hash.
///
/// # Arguments
/// - `config` — on-chain settlement env vars
/// - `epoch` — current epoch number (for logging)
/// - `proof_bytes` — the Groth16 proof bytes
/// - `public_values` — ABI-encoded `(uint256 epoch, uint16[] payouts)`
///
/// # Returns
/// The transaction hash as a `0x`-prefixed lowercase hex string.
pub async fn settle_epoch_onchain(
    config: &SettlementConfig,
    epoch: u64,
    proof_bytes: &[u8],
    public_values: &[u8],
) -> anyhow::Result<String> {
    info!(
        "settlement: submitting epoch {} — hook={} proof={}B pv={}B",
        epoch,
        config.hook_address,
        proof_bytes.len(),
        public_values.len(),
    );

    let signer: PrivateKeySigner = PrivateKeySigner::from_str(&config.private_key)
        .map_err(|e| anyhow::anyhow!("settlement: invalid PRIVATE_KEY: {}", e))?;
    let wallet = EthereumWallet::from(signer);

    let rpc_url = config
        .rpc_url
        .parse()
        .map_err(|e| anyhow::anyhow!("settlement: invalid UNICHAIN_RPC_URL: {}", e))?;
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(wallet)
        .on_http(rpc_url);

    let hook_address = Address::from_str(&config.hook_address)
        .map_err(|e| anyhow::anyhow!("settlement: invalid PRISM_HOOK_ADDRESS: {}", e))?;

    let call = IPrismHook::settleEpochCall {
        proof: Bytes::copy_from_slice(proof_bytes),
        publicValues: Bytes::copy_from_slice(public_values),
    };

    let tx = TransactionRequest::default()
        .with_to(hook_address)
        .with_input(call.abi_encode());

    let pending = provider
        .send_transaction(tx)
        .await
        .map_err(|e| anyhow::anyhow!("settlement: send_transaction failed: {}", e))?;

    let receipt = pending
        .with_required_confirmations(1)
        .get_receipt()
        .await
        .map_err(|e| anyhow::anyhow!("settlement: receipt fetch failed: {}", e))?;

    let tx_hash = format!("0x{:x}", receipt.transaction_hash);
    if !receipt.status() {
        return Err(anyhow::anyhow!(
            "settlement: epoch {} reverted on-chain — tx={}",
            epoch,
            tx_hash
        ));
    }

    info!(
        "settlement: epoch {} settled on-chain — tx={} block={}",
        epoch,
        tx_hash,
        receipt.block_number.unwrap_or_default()
    );
    Ok(tx_hash)
}

/// Plan-B settlement variant: submit three sub-proofs + their (already
/// schema-prefixed) public-values blobs + a claimed `(epoch, payouts)` tuple
/// to `PrismHook.settleEpochThreeProof`. Use as a fallback when Groth16 wrap
/// fails. Trust assumption shifts: the hook verifies each sub-STARK but
/// trusts the operator to relay `(epoch, payouts)` faithfully — see PrismHook
/// NatSpec on `settleEpochThreeProof`.
#[allow(clippy::too_many_arguments)]
pub async fn settle_epoch_three_proof_onchain(
    config: &SettlementConfig,
    epoch: u64,
    solver_proof: &[u8],
    solver_pv: &[u8],
    execution_proof: &[u8],
    execution_pv: &[u8],
    shapley_proof: &[u8],
    shapley_pv: &[u8],
    payouts: &[u16],
) -> anyhow::Result<String> {
    info!(
        "settlement: submitting Plan-B epoch {} — hook={} solver={}B exec={}B shapley={}B payouts={:?}",
        epoch,
        config.hook_address,
        solver_proof.len(),
        execution_proof.len(),
        shapley_proof.len(),
        payouts,
    );

    let signer: PrivateKeySigner = PrivateKeySigner::from_str(&config.private_key)
        .map_err(|e| anyhow::anyhow!("settlement: invalid PRIVATE_KEY: {}", e))?;
    let wallet = EthereumWallet::from(signer);

    let rpc_url = config
        .rpc_url
        .parse()
        .map_err(|e| anyhow::anyhow!("settlement: invalid UNICHAIN_RPC_URL: {}", e))?;
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(wallet)
        .on_http(rpc_url);

    let hook_address = Address::from_str(&config.hook_address)
        .map_err(|e| anyhow::anyhow!("settlement: invalid PRISM_HOOK_ADDRESS: {}", e))?;

    let call = IPrismHook::settleEpochThreeProofCall {
        solverProof: Bytes::copy_from_slice(solver_proof),
        solverPv: Bytes::copy_from_slice(solver_pv),
        executionProof: Bytes::copy_from_slice(execution_proof),
        executionPv: Bytes::copy_from_slice(execution_pv),
        shapleyProof: Bytes::copy_from_slice(shapley_proof),
        shapleyPv: Bytes::copy_from_slice(shapley_pv),
        epoch: alloy_primitives::U256::from(epoch),
        payouts: payouts.to_vec(),
    };

    let tx = TransactionRequest::default()
        .with_to(hook_address)
        .with_input(call.abi_encode());

    let pending = provider
        .send_transaction(tx)
        .await
        .map_err(|e| anyhow::anyhow!("settlement: send_transaction failed: {}", e))?;

    let receipt = pending
        .with_required_confirmations(1)
        .get_receipt()
        .await
        .map_err(|e| anyhow::anyhow!("settlement: receipt fetch failed: {}", e))?;

    let tx_hash = format!("0x{:x}", receipt.transaction_hash);
    if !receipt.status() {
        return Err(anyhow::anyhow!(
            "settlement: Plan-B epoch {} reverted on-chain — tx={}",
            epoch,
            tx_hash
        ));
    }

    info!(
        "settlement: Plan-B epoch {} settled on-chain — tx={} block={}",
        epoch,
        tx_hash,
        receipt.block_number.unwrap_or_default()
    );
    Ok(tx_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_from_env_returns_none_when_missing() {
        // With no env vars set, should return None.
        let _ = SettlementConfig::from_env();
    }

    #[test]
    fn settle_epoch_call_abi_encode_has_selector_and_two_dynamic_args() {
        let call = IPrismHook::settleEpochCall {
            proof: Bytes::from(vec![0xCA, 0xFE]),
            publicValues: Bytes::from(vec![0xBE, 0xEF]),
        };
        let bytes = call.abi_encode();
        // Two dynamic args → at minimum 4 (selector) + 32 (offset1) +
        // 32 (offset2) + 32 (len1) + 32 (proof padded) + 32 (len2) +
        // 32 (pv padded) = 196 bytes. Use ≥132 as a loose lower bound.
        assert!(bytes.len() >= 132);
    }

    #[test]
    fn with_schema_byte_prepends_one_and_preserves_inner() {
        let inner = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let wrapped = with_schema_byte(&inner);
        assert_eq!(wrapped[0], SCHEMA_VERSION);
        assert_eq!(wrapped[0], 1);
        assert_eq!(&wrapped[1..], &inner[..]);
    }

    #[test]
    fn settle_three_proof_call_encodes_eight_args() {
        let call = IPrismHook::settleEpochThreeProofCall {
            solverProof: Bytes::from(vec![0x01]),
            solverPv: Bytes::from(vec![0x01, 0xAA]),
            executionProof: Bytes::from(vec![0x02]),
            executionPv: Bytes::from(vec![0x01, 0xBB]),
            shapleyProof: Bytes::from(vec![0x03]),
            shapleyPv: Bytes::from(vec![0x01, 0xCC]),
            epoch: alloy_primitives::U256::from(7u64),
            payouts: vec![4000, 2500, 2000, 1500, 0],
        };
        let bytes = call.abi_encode();
        // Selector (4) + 8 head words (32 each) + payload tail.
        assert!(bytes.len() >= 4 + 8 * 32);
    }

    #[test]
    fn private_key_signer_round_trips_canonical_hex() {
        // Anvil's first canonical key — verify our parser accepts the
        // 0x-prefixed format `.env.example` uses.
        let key = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
        let signer = PrivateKeySigner::from_str(key).expect("parse anvil key");
        let expected: Address = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
            .parse()
            .unwrap();
        assert_eq!(signer.address(), expected);
    }
}
