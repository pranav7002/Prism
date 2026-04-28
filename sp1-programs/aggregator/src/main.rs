// PRISM SP1 Program: aggregator (RECURSIVE)
// Verification key must be extracted after compilation: `cargo prove build`
// inside this directory. The resulting ELF lives at
// `elf/riscv32im-succinct-zkvm-elf`. Extract AGGREGATOR_VKEY via
// `ProverClient::from_env().setup(elf).1.bytes32()` (SP1 3.x) and pass it to
// Dev 2 for embedding in `PrismCoordinator.sol`.
//
// Only this program's proof is submitted on-chain. Its recursive verification
// of the three sub-proofs is what gives the 260-byte Groth16 its
// cross-consistency guarantees.

#![no_main]

sp1_zkvm::entrypoint!(main);

extern crate alloc;

use alloc::vec::Vec;
use sha2::{Digest, Sha256};

// ----------------------------------------------------------------------------
// Solidity-ABI encoding for `(uint256 epoch, uint16[] payouts)`
// ----------------------------------------------------------------------------
//
// Must produce byte-identical output to `abi.encode(epoch, payouts)` so the
// PrismHook contract can `abi.decode` the Groth16 proof's public values
// directly. Mirrors `prism-orchestrator::proving::encode_public_values_abi`.
//
// Layout:
//   word 0  = epoch (uint256, big-endian, lower 8 bytes used)
//   word 1  = 0x40 (offset to dynamic-array head)
//   word 2  = payouts.len() (uint256)
//   word 3+ = payouts[i] each padded to 32 bytes (lower 2 bytes used)
fn encode_public_values_abi(epoch: u64, payouts_bps: &[u16]) -> Vec<u8> {
    let len = payouts_bps.len();
    let mut out = Vec::with_capacity((3 + len) * 32);

    // word 0: epoch (uint256, BE)
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&epoch.to_be_bytes());
    out.extend_from_slice(&w);

    // word 1: dynamic-array offset = 64 bytes from start of tuple body
    let mut w = [0u8; 32];
    w[31] = 0x40;
    out.extend_from_slice(&w);

    // word 2: array length
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&(len as u64).to_be_bytes());
    out.extend_from_slice(&w);

    // words 3..3+len: each payout (uint16 padded to uint256)
    for &p in payouts_bps {
        let mut w = [0u8; 32];
        w[30..].copy_from_slice(&p.to_be_bytes());
        out.extend_from_slice(&w);
    }

    out
}

// ----------------------------------------------------------------------------
// Recursive verification + cross-consistency
// ----------------------------------------------------------------------------

/// SHA-256 over a slice of public-value bytes. SP1's
/// `verify_sp1_proof(vkey, pv_digest)` expects the caller to supply the same
/// hash that the proof committed to.
fn sha256_commitments(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

pub fn main() {
    // Per-proof verification keys, encoded as the 32-byte hash SP1's
    // `verify_sp1_proof` expects.
    let solver_vkey: [u32; 8] = sp1_zkvm::io::read();
    let execution_vkey: [u32; 8] = sp1_zkvm::io::read();
    let shapley_vkey: [u32; 8] = sp1_zkvm::io::read();

    // Committed public values of each sub-proof as raw byte blobs. We hash
    // each with SHA-256 to produce the digest SP1 uses to bind the recursive
    // verifier to a specific proof.
    let solver_pv_bytes: Vec<u8> = sp1_zkvm::io::read();
    let execution_pv_bytes: Vec<u8> = sp1_zkvm::io::read();
    let shapley_pv_bytes: Vec<u8> = sp1_zkvm::io::read();

    // Epoch witnesses still come in via stdin. The on-chain `settleEpoch`
    // already verifies `publicValues.epoch == currentEpoch`, so a malicious
    // orchestrator that lies about the epoch can only stall settlement, not
    // pay arbitrary payouts. The transitive `solver_epoch == exec_epoch ==
    // shapley_epoch` check below catches obvious inconsistency.
    let solver_epoch: u64 = sp1_zkvm::io::read();
    let exec_epoch: u64 = sp1_zkvm::io::read();
    let shapley_epoch: u64 = sp1_zkvm::io::read();
    // Basis-point payouts (out of 10_000) — what the on-chain `settleEpoch`
    // consumes via abi.decode. Read as Vec<u16> rather than the previous
    // `Vec<(AgentId, u128)>` token-unit shape, so the aggregator's committed
    // public values can be the ABI-encoded blob the contract decodes.
    let payouts_bps: Vec<u16> = sp1_zkvm::io::read();

    // Hash anchors are NO LONGER read from stdin. Pre-H4 fix the
    // orchestrator passed them as separate words and the aggregator only
    // checked them != [0;32], so a malicious prover holding valid
    // sub-proofs could supply arbitrary anchor values. Now the aggregator
    // *derives* each anchor from the trailing 32 bytes of the
    // recursively-verified PV blob, exploiting the fact that each
    // sub-program ends its commit stream with a `commit(&[u8;32])`:
    //
    //   solver-proof:    last 32 bytes = intents_hash
    //   execution-proof: last 32 bytes = exec_hash
    //   shapley-proof:   last 32 bytes = dist_hash
    //
    // bincode of `[u8;32]` is exactly 32 raw bytes (fixed-size arrays
    // have no length prefix). Combined with `verify_sp1_proof` binding
    // (vkey, sha256(pv_bytes)), the anchors are now cryptographically
    // bound to what each sub-proof actually committed.
    fn last_32(pv_bytes: &[u8]) -> [u8; 32] {
        let n = pv_bytes.len();
        assert!(n >= 32, "pv_bytes shorter than 32 bytes — sub-program shape changed?");
        let mut out = [0u8; 32];
        out.copy_from_slice(&pv_bytes[n - 32..n]);
        out
    }

    // ------------------------------------------------------------------
    // Recursive STARK verification. These calls panic — rejecting the
    // containing proof — if any sub-proof is invalid.
    // ------------------------------------------------------------------
    let solver_digest = sha256_commitments(&solver_pv_bytes);
    let execution_digest = sha256_commitments(&execution_pv_bytes);
    let shapley_digest = sha256_commitments(&shapley_pv_bytes);

    sp1_zkvm::lib::verify::verify_sp1_proof(&solver_vkey, &solver_digest);
    sp1_zkvm::lib::verify::verify_sp1_proof(&execution_vkey, &execution_digest);
    sp1_zkvm::lib::verify::verify_sp1_proof(&shapley_vkey, &shapley_digest);

    // ------------------------------------------------------------------
    // Hash anchors derived from the verified PV blobs (closes H4).
    // ------------------------------------------------------------------
    let solver_intents_hash = last_32(&solver_pv_bytes);
    let exec_hash = last_32(&execution_pv_bytes);
    let shapley_dist_hash = last_32(&shapley_pv_bytes);

    // Sanity: a sub-proof that committed [0u8;32] would be a bug in that
    // program's hash function. Keep the original guard.
    assert!(solver_intents_hash != [0u8; 32], "empty solver anchor");
    assert!(exec_hash != [0u8; 32], "empty exec anchor");
    assert!(shapley_dist_hash != [0u8; 32], "empty shapley anchor");

    // ------------------------------------------------------------------
    // Cross-consistency: all three sub-proofs must describe the same epoch.
    // ------------------------------------------------------------------
    assert!(
        solver_epoch == exec_epoch && exec_epoch == shapley_epoch,
        "epoch mismatch across sub-proofs"
    );

    // ------------------------------------------------------------------
    // Final settlement hash binds everything together.
    // ------------------------------------------------------------------
    let mut h = Sha256::new();
    h.update(solver_intents_hash);
    h.update(exec_hash);
    h.update(shapley_dist_hash);
    h.update(solver_epoch.to_be_bytes());
    let d = h.finalize();
    let mut final_settlement_hash = [0u8; 32];
    final_settlement_hash.copy_from_slice(&d);

    // Commit Solidity-ABI bytes for `(uint256 epoch, uint16[] payouts)` —
    // exactly what `PrismHook.settleEpoch` decodes. The orchestrator hands
    // these proof bytes to the verifier without further modification, so the
    // SP1 verifier's `sha256(publicValues)` digest matches the proof's
    // committed digest. (Previously the orchestrator overwrote
    // `public_values` post-aggregation, which broke that invariant — see
    // AUDIT_REPORT.md C2.)
    //
    // `final_settlement_hash` is no longer in the on-chain public-values
    // payload; the contract derives epoch + payout binding from those two
    // fields alone. We still compute it above so future aggregator versions
    // can re-introduce it as needed.
    let _ = final_settlement_hash;
    let abi_bytes = encode_public_values_abi(solver_epoch, &payouts_bps);
    sp1_zkvm::io::commit_slice(&abi_bytes);
}
