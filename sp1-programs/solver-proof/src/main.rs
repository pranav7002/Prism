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
