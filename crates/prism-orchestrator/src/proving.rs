//! Parallel Groth16 proving pipeline.
//!
//! This module drives the SP1 prover SDK to generate all four proofs for an
//! epoch. The three base proofs (solver / execution / shapley) run in
//! parallel via `tokio::join!` + `spawn_blocking`; the aggregator waits on
//! all three and produces a single Groth16 proof suitable for on-chain
//! verification.
//!
//! Two paths:
//! - Mock path (`real_prover = None`): returns deterministic 128-byte zeroed
//!   proofs without calling SP1. Lets the orchestrator smoke-test
//!   end-to-end without the SP1 toolchain installed.
//! - Real path (`real_prover = Some(...)`, requires the `real-prover`
//!   feature): invokes `sp1_sdk::ProverClient::new()` (which honors
//!   `SP1_PROVER=network` + `SP1_PRIVATE_KEY=...`) and executes the real
//!   pipeline. Requires the RISC-V ELFs to have been produced by
//!   `cargo prove build` in each `sp1-programs/*/`.

use std::sync::Arc;

use prism_solver::build_execution_plan;
use prism_types::{AgentId, AgentIntent, HealthFactor, ProtocolState, WsEvent};
use tokio::sync::broadcast;
use tracing::info;

#[cfg(feature = "real-prover")]
use sp1_sdk::{HashableKey, ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin, SP1VerifyingKey, SP1Proof};

// ---------------------------------------------------------------------------
// ELF inclusion
// ---------------------------------------------------------------------------
//
// `mock-elf` (default) swaps real ELFs for empty slices so the crate compiles
// without `cargo prove build` output. When producing real proofs, build with
// `--no-default-features --features real-prover`.

#[cfg(feature = "mock-elf")]
pub const SOLVER_ELF: &[u8] = &[];
#[cfg(feature = "mock-elf")]
pub const EXECUTION_ELF: &[u8] = &[];
#[cfg(feature = "mock-elf")]
pub const SHAPLEY_ELF: &[u8] = &[];
#[cfg(feature = "mock-elf")]
pub const AGGREGATOR_ELF: &[u8] = &[];

// SP1 v3.x toolchain (pinned to match sp1-sdk = "3.0.0") emits 32-bit
// RISC-V ELFs at `sp1-programs/<name>/elf/riscv32im-succinct-zkvm-elf`.
#[cfg(not(feature = "mock-elf"))]
pub const SOLVER_ELF: &[u8] = include_bytes!(
    "../../../sp1-programs/solver-proof/elf/riscv32im-succinct-zkvm-elf"
);
#[cfg(not(feature = "mock-elf"))]
pub const EXECUTION_ELF: &[u8] = include_bytes!(
    "../../../sp1-programs/execution-proof/elf/riscv32im-succinct-zkvm-elf"
);
#[cfg(not(feature = "mock-elf"))]
pub const SHAPLEY_ELF: &[u8] = include_bytes!(
    "../../../sp1-programs/shapley-proof/elf/riscv32im-succinct-zkvm-elf"
);
#[cfg(not(feature = "mock-elf"))]
pub const AGGREGATOR_ELF: &[u8] = include_bytes!(
    "../../../sp1-programs/aggregator/elf/riscv32im-succinct-zkvm-elf"
);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub struct ProverConfig {
    pub use_mock_prover: bool,
    // ELF bytes are read only on the real-prover path.
    #[allow(dead_code)]
    pub solver_elf: &'static [u8],
    #[allow(dead_code)]
    pub execution_elf: &'static [u8],
    #[allow(dead_code)]
    pub shapley_elf: &'static [u8],
    pub aggregator_elf: &'static [u8],
}

impl ProverConfig {
    pub fn from_compiled(use_mock_prover: bool) -> Self {
        Self {
            use_mock_prover,
            solver_elf: SOLVER_ELF,
            execution_elf: EXECUTION_ELF,
            shapley_elf: SHAPLEY_ELF,
            aggregator_elf: AGGREGATOR_ELF,
        }
    }
}

pub struct AggregatedProof {
    pub proof_bytes: Vec<u8>,
    pub public_values: Vec<u8>,
    /// Shapley weights from the solver's execution plan, one per agent.
    pub shapley_weights: Vec<(AgentId, u16)>,
}

// ---------------------------------------------------------------------------
// Real prover (feature-gated)
// ---------------------------------------------------------------------------

/// Cached ELF byte references for the real-prover path. Construct once and
/// share across epochs.
#[cfg(feature = "real-prover")]
pub struct Elfs<'a> {
    pub solver: &'a [u8],
    pub execution: &'a [u8],
    pub shapley: &'a [u8],
    pub aggregator: &'a [u8],
}

/// Holds the long-lived `ProverClient` plus the four (pk, vk) pairs. Setting
/// up SP1 is expensive — share an `Arc<RealProver>` across epoch loops.
#[cfg(feature = "real-prover")]
pub struct RealProver {
    pub client: ProverClient,
    pub solver_pk: SP1ProvingKey,
    pub solver_vk: SP1VerifyingKey,
    pub execution_pk: SP1ProvingKey,
    pub execution_vk: SP1VerifyingKey,
    pub shapley_pk: SP1ProvingKey,
    pub shapley_vk: SP1VerifyingKey,
    pub aggregator_pk: SP1ProvingKey,
    pub aggregator_vk: SP1VerifyingKey,
}

#[cfg(feature = "real-prover")]
impl RealProver {
    /// Build a `RealProver` from the four ELFs. Reads `SP1_PROVER` (e.g.
    /// `network`) and, for `network`, `SP1_PRIVATE_KEY` from the environment.
    pub fn new(elfs: &Elfs<'_>) -> anyhow::Result<Self> {
        let client = ProverClient::new();
        let (solver_pk, solver_vk) = client.setup(elfs.solver);
        let (execution_pk, execution_vk) = client.setup(elfs.execution);
        let (shapley_pk, shapley_vk) = client.setup(elfs.shapley);
        let (aggregator_pk, aggregator_vk) = client.setup(elfs.aggregator);
        Ok(Self {
            client,
            solver_pk,
            solver_vk,
            execution_pk,
            execution_vk,
            shapley_pk,
            shapley_vk,
            aggregator_pk,
            aggregator_vk,
        })
    }
}

// Type-erased Option<Arc<RealProver>> for the default-features build. Lets
// `prove_epoch`'s signature be feature-stable without forcing `main.rs` to
// branch on `cfg`.
#[cfg(not(feature = "real-prover"))]
pub struct RealProver;

// ---------------------------------------------------------------------------
// ChildProofArtifact: shared shape for both mock + real paths.
// ---------------------------------------------------------------------------

struct ChildProofArtifact {
    /// The actual SDK proof. `None` on the mock path.
    #[cfg(feature = "real-prover")]
    proof: Option<SP1ProofWithPublicValues>,
    /// Verifying key digest as `[u32; 8]` (input to `verify_sp1_proof` in the
    /// recursive aggregator). All zeros on the mock path.
    vk_hash: [u32; 8],
    /// Raw bincode bytes of the program's committed public values. Empty on
    /// the mock path.
    pv_bytes: Vec<u8>,
    /// Anchor hash committed by the program (intents / exec / dist hash).
    proof_hash: [u8; 32],
    /// Epoch this artifact corresponds to (read from PV on the real path).
    epoch: u64,
    /// Shapley payouts. Only the shapley artifact populates this; others empty.
    payouts: Vec<(AgentId, u128)>,
    /// Stub bytes — only populated on mock path so the existing tests + the
    /// non-recursive sanity checks still see "something".
    #[allow(dead_code)]
    stub_bytes: Vec<u8>,
}

impl ChildProofArtifact {
    fn mock_for(seed: &[u8]) -> Self {
        Self {
            #[cfg(feature = "real-prover")]
            proof: None,
            vk_hash: [0u32; 8],
            pv_bytes: Vec::new(),
            proof_hash: [0u8; 32],
            epoch: 0,
            payouts: Vec::new(),
            stub_bytes: mock_prove(&[seed]),
        }
    }
}
