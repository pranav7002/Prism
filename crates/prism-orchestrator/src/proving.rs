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

// ---------------------------------------------------------------------------
// Drive an epoch's pipeline
// ---------------------------------------------------------------------------

/// Drive the four-program pipeline for one epoch.
pub async fn prove_epoch(
    config: &ProverConfig,
    real_prover: Option<Arc<RealProver>>,
    intents: Vec<AgentIntent>,
    protocol_state: ProtocolState,
    health_factor: HealthFactor,
    event_tx: broadcast::Sender<WsEvent>,
) -> anyhow::Result<AggregatedProof> {
    // Solver phase.
    let _ = event_tx.send(WsEvent::SolverRunning {
        conflicts_detected: 0,
    });
    let plan = build_execution_plan(intents.clone(), &protocol_state)
        .map_err(|e| anyhow::anyhow!("solver failed: {}", e))?;
    let plan_hash = hash_plan(&plan);
    let _ = event_tx.send(WsEvent::SolverComplete {
        plan_hash: format!("0x{}", hex::encode(plan_hash)),
        dropped: vec![],
    });

    // Capture the solver's actual Shapley weights before cloning the plan.
    let shapley_weights = plan.shapley_weights.clone();
    let plan_epoch = plan.epoch;

    emit(&event_tx, "solver", 10);

    // Three base proofs in parallel.
    let solver_task = run_solver_task(
        config,
        real_prover.clone(),
        intents.clone(),
        protocol_state.clone(),
        plan.clone(),
        event_tx.clone(),
    );
    let execution_task = run_execution_task(
        config,
        real_prover.clone(),
        plan.clone(),
        protocol_state.clone(),
        health_factor.clone(),
        event_tx.clone(),
    );
    let shapley_task = run_shapley_task(
        config,
        real_prover.clone(),
        plan.clone(),
        event_tx.clone(),
    );

    let (solver_res, execution_res, shapley_res) =
        tokio::join!(solver_task, execution_task, shapley_task);

    let solver_artifact = solver_res?;
    let execution_artifact = execution_res?;
    let shapley_artifact = shapley_res?;

    // Aggregation phase.
    let _ = event_tx.send(WsEvent::AggregationStart);
    emit(&event_tx, "aggregator", 80);
    let agg_start = std::time::Instant::now();

    // ABI-encode publicValues as (uint256 epoch, uint16[] payouts) so the
    // on-chain PrismHook.settleEpoch can abi.decode them directly.
    let payouts_bps: Vec<u16> = shapley_weights.iter().map(|(_, w)| *w).collect();
    let public_values = encode_public_values_abi(plan_epoch, &payouts_bps);

    let aggregated = match real_prover.as_ref() {
        Some(rp) => {
            #[cfg(feature = "real-prover")]
            {
                let rp = rp.clone();
                let mut agg = run_aggregator_real(
                    rp,
                    solver_artifact,
                    execution_artifact,
                    shapley_artifact,
                )
                .await?;
                agg.public_values = public_values;
                agg.shapley_weights = shapley_weights.clone();
                agg
            }
            #[cfg(not(feature = "real-prover"))]
            {
                // Compiled without `real-prover` feature — `RealProver` is a
                // unit ZST and cannot actually drive SP1. Fall through to mock.
                let _ = rp;
                AggregatedProof {
                    proof_bytes: mock_prove(&[
                        &solver_artifact.stub_bytes,
                        &execution_artifact.stub_bytes,
                        &shapley_artifact.stub_bytes,
                    ]),
                    public_values,
                    shapley_weights: shapley_weights.clone(),
                }
            }
        }
        None => AggregatedProof {
            proof_bytes: mock_prove(&[
                &solver_artifact.stub_bytes,
                &execution_artifact.stub_bytes,
                &shapley_artifact.stub_bytes,
            ]),
            public_values,
            shapley_weights: shapley_weights.clone(),
        },
    };

    let agg_ms = agg_start.elapsed().as_millis() as u64;
    emit(&event_tx, "aggregator", 100);
    let _ = event_tx.send(WsEvent::AggregationComplete { time_ms: agg_ms });

    // Groth16 wrap progression (the Groth16 wrap happens inside the
    // aggregator's `.groth16().run()` SDK call; we fire synthetic progress
    // ticks so the frontend animation has something to render).
    for &pct in &[25u8, 50, 75, 100] {
        let _ = event_tx.send(WsEvent::Groth16Wrapping { pct });
    }

    info!(
        "prove_epoch finished; proof {} bytes, pv {} bytes, {} agents, agg {} ms",
        aggregated.proof_bytes.len(),
        aggregated.public_values.len(),
        aggregated.shapley_weights.len(),
        agg_ms,
    );
    Ok(aggregated)
}

fn hash_plan(plan: &prism_types::ExecutionPlan) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(plan.epoch.to_be_bytes());
    for i in &plan.ordered_intents {
        h.update(i.commitment);
    }
    h.update(plan.cooperative_mev_value.to_be_bytes());
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

// ---------------------------------------------------------------------------
// Task runners
// ---------------------------------------------------------------------------

async fn run_solver_task(
    config: &ProverConfig,
    real_prover: Option<Arc<RealProver>>,
    intents: Vec<AgentIntent>,
    protocol_state: ProtocolState,
    plan: prism_types::ExecutionPlan,
    event_tx: broadcast::Sender<WsEvent>,
) -> anyhow::Result<ChildProofArtifact> {
    run_with_progress(config, "solver", real_prover, event_tx, {
        let intents = intents.clone();
        move || {
            let mut payload = Vec::with_capacity(64);
            for intent in &intents {
                payload.extend_from_slice(&intent.commitment);
            }
            Ok(ChildProofArtifact::mock_for(&payload))
        }
    }, {
        // Real-prover closure. Constructs SP1Stdin and calls
        // `client.prove(...).compressed().run()` on a blocking thread.
        #[cfg(feature = "real-prover")]
        {
            let intents = intents.clone();
            let protocol_state = protocol_state.clone();
            let plan = plan.clone();
            move |rp: Arc<RealProver>| -> anyhow::Result<ChildProofArtifact> {
                let mut stdin = SP1Stdin::new();
                stdin.write(&intents);
                stdin.write(&protocol_state);
                stdin.write(&plan);
                let proof = rp
                    .client
                    .prove(&rp.solver_pk, stdin)
                    .compressed()
                    .run()?;
                let pv_bytes = proof.public_values.to_vec();
                let mut pv = proof.public_values.clone();
                let _committed_plan: prism_types::ExecutionPlan = pv.read();
                let intents_hash: [u8; 32] = pv.read();
                Ok(ChildProofArtifact {
                    proof: Some(proof),
                    vk_hash: rp.solver_vk.hash_u32(),
                    pv_bytes,
                    proof_hash: intents_hash,
                    epoch: plan.epoch,
                    payouts: Vec::new(),
                    stub_bytes: Vec::new(),
                })
            }
        }
        #[cfg(not(feature = "real-prover"))]
        {
            let _ = (intents, protocol_state, plan);
            move |_rp: Arc<RealProver>| -> anyhow::Result<ChildProofArtifact> {
                unreachable!("real-prover path unreachable without feature")
            }
        }
    })
    .await
}

async fn run_execution_task(
    config: &ProverConfig,
    real_prover: Option<Arc<RealProver>>,
    plan: prism_types::ExecutionPlan,
    protocol_state: ProtocolState,
    health_factor: HealthFactor,
    event_tx: broadcast::Sender<WsEvent>,
) -> anyhow::Result<ChildProofArtifact> {
    run_with_progress(config, "execution", real_prover, event_tx, {
        let plan = plan.clone();
        move || Ok(ChildProofArtifact::mock_for(&plan.epoch.to_be_bytes()))
    }, {
        #[cfg(feature = "real-prover")]
        {
            let plan = plan.clone();
            let protocol_state = protocol_state.clone();
            let health_factor = health_factor.clone();
            move |rp: Arc<RealProver>| -> anyhow::Result<ChildProofArtifact> {
                let mut stdin = SP1Stdin::new();
                stdin.write(&plan);
                stdin.write(&protocol_state);
                stdin.write(&health_factor);
                let proof = rp
                    .client
                    .prove(&rp.execution_pk, stdin)
                    .compressed()
                    .run()?;
                let pv_bytes = proof.public_values.to_vec();
                let mut pv = proof.public_values.clone();
                let _valid: bool = pv.read();
                let _gas: u128 = pv.read();
                let exec_hash: [u8; 32] = pv.read();
                Ok(ChildProofArtifact {
                    proof: Some(proof),
                    vk_hash: rp.execution_vk.hash_u32(),
                    pv_bytes,
                    proof_hash: exec_hash,
                    epoch: plan.epoch,
                    payouts: Vec::new(),
                    stub_bytes: Vec::new(),
                })
            }
        }
        #[cfg(not(feature = "real-prover"))]
        {
            let _ = (plan, protocol_state, health_factor);
            move |_rp: Arc<RealProver>| -> anyhow::Result<ChildProofArtifact> {
                unreachable!("real-prover path unreachable without feature")
            }
        }
    })
    .await
}

