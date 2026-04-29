// PRISM SP1 Program: solver-proof
// Verification key must be extracted after compilation: `cargo prove build`
// inside this directory. The resulting ELF lives at
// `elf/riscv32im-succinct-zkvm-elf`.
//
// Purpose: proves the off-chain solver produced its ExecutionPlan honestly.
// Constraints enforced (panic == no valid proof):
//   1. Commitment binding — each intent's commitment matches its fields.
//   2. No-fabrication — len(ordered) == len(input).
//   3. Priority ordering — non-increasing, with β-before-δ exception allowed.
//   4. Backrun safety — every Backrun has a non-zero target tx hash.
//   5. KillSwitch-first — if any KillSwitch exists, it sits at index 0.
//
// Duplicated types (kept field-compatible with prism-types::*).

#![no_main]

sp1_zkvm::entrypoint!(main);

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tiny_keccak::{Hasher, Keccak};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct AgentId([u8; 20]);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ConsolidateRemove {
    pool: [u8; 20],
    liquidity: u128,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ConsolidateAdd {
    pool: [u8; 20],
    amount0: u128,
    amount1: u128,
    tick_lower: i32,
    tick_upper: i32,
}

// Externally-tagged (serde default). Must match `prism-types::Action` bit-for-bit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Action {
    Swap {
        pool: [u8; 20],
        token_in: [u8; 20],
        token_out: [u8; 20],
        amount_in: u128,
        min_out: u128,
    },
    AddLiquidity {
        pool: [u8; 20],
        amount0: u128,
        amount1: u128,
        tick_lower: i32,
        tick_upper: i32,
    },
    RemoveLiquidity {
        pool: [u8; 20],
        liquidity: u128,
    },
    Backrun {
        target_tx: [u8; 32],
        profit_token: [u8; 20],
    },
    DeltaHedge {
        position_id: u64,
        delta: i64,
    },
    MigrateLiquidity {
        from_pool: [u8; 20],
        to_pool: [u8; 20],
        amount: u128,
        tick_lower: i32,
        tick_upper: i32,
    },
    BatchConsolidate {
        removes: Vec<ConsolidateRemove>,
        adds: Vec<ConsolidateAdd>,
    },
    SetDynamicFee {
        pool: [u8; 20],
        new_fee_ppm: u32,
    },
    CrossProtocolHedge {
        aave_borrow_asset: [u8; 20],
        aave_borrow_amount: u128,
        uniswap_pool: [u8; 20],
        uniswap_token_in: [u8; 20],
        uniswap_token_out: [u8; 20],
        uniswap_amount_in: u128,
    },
    KillSwitch {
        reason: String,
    },
}

impl Action {
    fn discriminator(&self) -> u8 {
        match self {
            Action::Swap { .. } => 0x01,
            Action::AddLiquidity { .. } => 0x02,
            Action::RemoveLiquidity { .. } => 0x03,
            Action::Backrun { .. } => 0x04,
            Action::DeltaHedge { .. } => 0x05,
            Action::MigrateLiquidity { .. } => 0x06,
            Action::BatchConsolidate { .. } => 0x07,
            Action::SetDynamicFee { .. } => 0x08,
            Action::CrossProtocolHedge { .. } => 0x09,
            Action::KillSwitch { .. } => 0xFF,
        }
    }

    fn encode_packed(&self, out: &mut Vec<u8>) {
        out.push(self.discriminator());
        match self {
            Action::Swap {
                pool,
                token_in,
                token_out,
                amount_in,
                min_out,
            } => {
                out.extend_from_slice(pool);
                out.extend_from_slice(token_in);
                out.extend_from_slice(token_out);
                out.extend_from_slice(&amount_in.to_be_bytes());
                out.extend_from_slice(&min_out.to_be_bytes());
            }
            Action::AddLiquidity {
                pool,
                amount0,
                amount1,
                tick_lower,
                tick_upper,
            } => {
                out.extend_from_slice(pool);
                out.extend_from_slice(&amount0.to_be_bytes());
                out.extend_from_slice(&amount1.to_be_bytes());
                out.extend_from_slice(&tick_lower.to_be_bytes());
                out.extend_from_slice(&tick_upper.to_be_bytes());
            }
            Action::RemoveLiquidity { pool, liquidity } => {
                out.extend_from_slice(pool);
                out.extend_from_slice(&liquidity.to_be_bytes());
            }
            Action::Backrun { target_tx, profit_token } => {
                out.extend_from_slice(target_tx);
                out.extend_from_slice(profit_token);
            }
            Action::DeltaHedge { position_id, delta } => {
                out.extend_from_slice(&position_id.to_be_bytes());
                out.extend_from_slice(&delta.to_be_bytes());
            }
            Action::MigrateLiquidity {
                from_pool,
                to_pool,
                amount,
                tick_lower,
                tick_upper,
            } => {
                out.extend_from_slice(from_pool);
                out.extend_from_slice(to_pool);
                out.extend_from_slice(&amount.to_be_bytes());
                out.extend_from_slice(&tick_lower.to_be_bytes());
                out.extend_from_slice(&tick_upper.to_be_bytes());
            }
            Action::BatchConsolidate { removes, adds } => {
                out.extend_from_slice(&(removes.len() as u32).to_be_bytes());
                for r in removes {
                    out.extend_from_slice(&r.pool);
                    out.extend_from_slice(&r.liquidity.to_be_bytes());
                }
                out.extend_from_slice(&(adds.len() as u32).to_be_bytes());
                for a in adds {
                    out.extend_from_slice(&a.pool);
                    out.extend_from_slice(&a.amount0.to_be_bytes());
                    out.extend_from_slice(&a.amount1.to_be_bytes());
                    out.extend_from_slice(&a.tick_lower.to_be_bytes());
                    out.extend_from_slice(&a.tick_upper.to_be_bytes());
                }
            }
            Action::SetDynamicFee { pool, new_fee_ppm } => {
                out.extend_from_slice(pool);
                out.extend_from_slice(&new_fee_ppm.to_be_bytes());
            }
            Action::CrossProtocolHedge {
                aave_borrow_asset,
                aave_borrow_amount,
                uniswap_pool,
                uniswap_token_in,
                uniswap_token_out,
                uniswap_amount_in,
            } => {
                out.extend_from_slice(aave_borrow_asset);
                out.extend_from_slice(&aave_borrow_amount.to_be_bytes());
                out.extend_from_slice(uniswap_pool);
                out.extend_from_slice(uniswap_token_in);
                out.extend_from_slice(uniswap_token_out);
                out.extend_from_slice(&uniswap_amount_in.to_be_bytes());
            }
            Action::KillSwitch { reason } => {
                let bytes = reason.as_bytes();
                out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
                out.extend_from_slice(bytes);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct AgentIntent {
    agent_id: AgentId,
    epoch: u64,
    target_protocol: String,
    action: Action,
    priority: u8,
    max_slippage_bps: u16,
    salt: [u8; 32],
    commitment: [u8; 32],
}

impl AgentIntent {
    fn compute_commitment(&self) -> [u8; 32] {
        let mut buf: Vec<u8> = Vec::with_capacity(128);
        buf.extend_from_slice(&self.agent_id.0);
        buf.extend_from_slice(&self.epoch.to_be_bytes());
        let proto = self.target_protocol.as_bytes();
        buf.extend_from_slice(&(proto.len() as u32).to_be_bytes());
        buf.extend_from_slice(proto);
        self.action.encode_packed(&mut buf);
        buf.push(self.priority);
        buf.extend_from_slice(&self.max_slippage_bps.to_be_bytes());
        buf.extend_from_slice(&self.salt);

        let mut hasher = Keccak::v256();
        hasher.update(&buf);
        let mut out = [0u8; 32];
        hasher.finalize(&mut out);
        out
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ExecutionPlan {
    epoch: u64,
    ordered_intents: Vec<AgentIntent>,
    cooperative_mev_value: u128,
    shapley_weights: Vec<(AgentId, u16)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ProtocolState {
    pool_address: [u8; 20],
    sqrt_price_x96: u128,
    liquidity: u128,
    tick: i32,
    fee_tier: u32,
    token0_reserve: u128,
    token1_reserve: u128,
    volatility_30d_bps: u32,
}

// ----------------------------------------------------------------------------

fn hash_intents(intents: &[AgentIntent]) -> [u8; 32] {
    let mut h = Sha256::new();
    for intent in intents {
        h.update(intent.agent_id.0);
        h.update(intent.epoch.to_be_bytes());
        h.update([intent.priority]);
        h.update(intent.commitment);
    }
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

fn is_backrun(a: &Action) -> bool {
    matches!(a, Action::Backrun { .. })
}

fn is_delta(a: &Action) -> bool {
    matches!(a, Action::DeltaHedge { .. })
}

fn is_killswitch(a: &Action) -> bool {
    matches!(a, Action::KillSwitch { .. })
}

pub fn main() {
    let input_intents: Vec<AgentIntent> = sp1_zkvm::io::read();
    let _protocol_state: ProtocolState = sp1_zkvm::io::read();
    let plan: ExecutionPlan = sp1_zkvm::io::read();

    // Constraint 0: non-empty input. An empty input would pass commitment +
    // ordering trivially and produce a non-empty proof for an empty epoch;
    // the aggregator's intents_hash != [0;32] check doesn't catch this
    // because sha256("") is non-zero. Reject inside the zkVM (M4).
    assert!(
        !input_intents.is_empty(),
        "solver-proof requires at least one input intent"
    );

    // Constraint 1: commitment binding.
    for intent in &input_intents {
        assert!(
            intent.compute_commitment() == intent.commitment,
            "commitment mismatch"
        );
    }

    // Constraint 2: no fabrication. Plan contains exactly the same
    // multiset of intents (by commitment hash) as the input.
    assert!(
        plan.ordered_intents.len() == input_intents.len(),
        "plan len != input len"
    );
    {
        let mut input_commitments: Vec<[u8; 32]> =
            input_intents.iter().map(|i| i.commitment).collect();
        let mut plan_commitments: Vec<[u8; 32]> =
            plan.ordered_intents.iter().map(|i| i.commitment).collect();
        input_commitments.sort();
        plan_commitments.sort();
        assert!(
            input_commitments == plan_commitments,
            "plan intents != input intents"
        );
    }

    // Constraint 5: KillSwitch-first.
    let has_killswitch = plan
        .ordered_intents
        .iter()
        .any(|i| is_killswitch(&i.action));
    if has_killswitch {
        assert!(
            is_killswitch(&plan.ordered_intents[0].action),
            "KillSwitch must be first"
        );
    }

    // Constraint 3: priority ordering, with two carve-outs: (a) KillSwitch
    // sits ahead of higher-priority items legitimately; (b) β-before-δ may
    // place a Backrun immediately before a strictly-higher-priority
    // DeltaHedge.
    for i in 0..plan.ordered_intents.len().saturating_sub(1) {
        let a = &plan.ordered_intents[i];
        let b = &plan.ordered_intents[i + 1];

        // Skip the KillSwitch first-slot carve-out.
        if i == 0 && is_killswitch(&a.action) {
            continue;
        }

        if a.priority >= b.priority {
            continue;
        }

        // Priority inversion — must be justified by β-before-δ.
        let inversion_ok = is_backrun(&a.action) && is_delta(&b.action);
        assert!(inversion_ok, "unauthorized priority inversion at index {}", i);
    }

    // Constraint 4: Backrun target_tx is non-zero.
    for intent in &plan.ordered_intents {
        if let Action::Backrun { target_tx, .. } = &intent.action {
            assert!(
                target_tx.iter().any(|b| *b != 0),
                "Backrun target_tx is zero"
            );
        }
    }

    // Commit outputs: the validated plan + a cross-consistency hash of the
    // sorted intents list.
    let intents_hash = hash_intents(&plan.ordered_intents);
    sp1_zkvm::io::commit(&plan);
    sp1_zkvm::io::commit(&intents_hash);
}
