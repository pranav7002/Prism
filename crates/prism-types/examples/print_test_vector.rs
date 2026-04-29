//! Golden commitment vectors for cross-language parity testing.
//!
//! Run:
//!   cargo run --example print_test_vector -p prism-types
//!
//! Each emit-line is a deterministic Action variant — same inputs each
//! run, so the output hex is stable. Pin these in Dev 3's Python tests
//! to detect any drift in the keccak-packed encoding.
//!
//! Coverage: all 10 Action variants (closes M9 in Audit report).

use prism_types::{Action, AgentId, AgentIntent, ConsolidateAdd, ConsolidateRemove};

fn salt_with(byte0: u8, epoch: u64) -> [u8; 32] {
    let mut s = [0u8; 32];
    s[0] = byte0;
    s[24..32].copy_from_slice(&epoch.to_be_bytes());
    s
}

fn main() {
    // 0x01 — Swap
    let i = AgentIntent::new_with_commitment(
        AgentId([0xAA; 20]),
        42,
        "Uniswap".into(),
        Action::Swap {
            pool: [0xDD; 20],
            token_in: [0x11; 20],
            token_out: [0x22; 20],
            amount_in: 1_000_000_000_000_000_000u128,
            min_out: 330_000_000_000_000_000u128,
        },
        80,
        50,
        [0x55; 32],
    );
    println!("SWAP_COMMITMENT=0x{}", hex::encode(i.commitment));

    // 0x02 — AddLiquidity
    let i = AgentIntent::new_with_commitment(
        AgentId([0xA0; 20]),
        1,
        "Uniswap".into(),
        Action::AddLiquidity {
            pool: [
                0x8a, 0xd5, 0x99, 0xc3, 0xa0, 0xff, 0x1d, 0xe0, 0x82, 0x01,
                0x1e, 0xfd, 0xdc, 0x58, 0xf1, 0x90, 0x8e, 0xb6, 0xe6, 0xd8,
            ],
            amount0: 300_000_000_000u128,
            amount1: 100_000_000_000_000_000_000u128,
            tick_lower: 196_800,
            tick_upper: 200_400,
        },
        70,
        50,
        salt_with(0, 1),
    );
    println!("ADD_LIQ_COMMITMENT=0x{}", hex::encode(i.commitment));

    // 0x03 — RemoveLiquidity
    let i = AgentIntent::new_with_commitment(
        AgentId([0xA1; 20]),
        2,
        "Uniswap".into(),
        Action::RemoveLiquidity {
            pool: [0xDE; 20],
            liquidity: 50_000_000_000u128,
        },
        65,
        100,
        salt_with(1, 2),
    );
    println!("REMOVE_LIQ_COMMITMENT=0x{}", hex::encode(i.commitment));

    // 0x04 — Backrun
    let i = AgentIntent::new_with_commitment(
        AgentId([0xA2; 20]),
        2,
        "Uniswap".into(),
        Action::Backrun {
            target_tx: [0xBE; 32],
            profit_token: [0x11; 20],
        },
        90,
        200,
        salt_with(2, 2),
    );
    println!("BACKRUN_COMMITMENT=0x{}", hex::encode(i.commitment));

    // 0x05 — DeltaHedge
    let i = AgentIntent::new_with_commitment(
        AgentId([0xA3; 20]),
        2,
        "Uniswap".into(),
        Action::DeltaHedge {
            position_id: 0xCAFEBABE_DEADBEEFu64,
            delta: -123_456_789_012_345i64,
        },
        40,
        50,
        salt_with(3, 2),
    );
    println!("DELTA_HEDGE_COMMITMENT=0x{}", hex::encode(i.commitment));

    // 0x06 — MigrateLiquidity
    let i = AgentIntent::new_with_commitment(
        AgentId([0xA5; 20]),
        2,
        "Uniswap".into(),
        Action::MigrateLiquidity {
            from_pool: [0xCC; 20],
            to_pool: [0xEE; 20],
            amount: 200_000_000_000u128,
            tick_lower: 200_400,
            tick_upper: 203_400,
        },
        75,
        75,
        salt_with(4, 2),
    );
    println!("MIGRATE_LIQ_COMMITMENT=0x{}", hex::encode(i.commitment));

    // 0x07 — BatchConsolidate
    let i = AgentIntent::new_with_commitment(
        AgentId([0xA6; 20]),
        2,
        "Uniswap".into(),
        Action::BatchConsolidate {
            removes: vec![
                ConsolidateRemove {
                    pool: [0x10; 20],
                    liquidity: 15_000_000_000u128,
                },
                ConsolidateRemove {
                    pool: [0x20; 20],
                    liquidity: 30_000_000_000u128,
                },
            ],
            adds: vec![ConsolidateAdd {
                pool: [0x30; 20],
                amount0: 10_000_000_000u128,
                amount1: 5_000_000_000_000_000_000u128,
                tick_lower: 199_800,
                tick_upper: 201_000,
            }],
        },
        55,
        100,
        salt_with(5, 2),
    );
    println!("BATCH_CONSOLIDATE_COMMITMENT=0x{}", hex::encode(i.commitment));

    // 0x08 — SetDynamicFee
    let i = AgentIntent::new_with_commitment(
        AgentId([0xA7; 20]),
        1,
        "Uniswap".into(),
        Action::SetDynamicFee {
            pool: [0xDD; 20],
            new_fee_ppm: 6_000,
        },
        65,
        20,
        salt_with(6, 1),
    );
    println!("SET_DYNAMIC_FEE_COMMITMENT=0x{}", hex::encode(i.commitment));

    // 0x09 — CrossProtocolHedge
    let i = AgentIntent::new_with_commitment(
        AgentId([0xA8; 20]),
        3,
        "Uniswap".into(),
        Action::CrossProtocolHedge {
            aave_borrow_asset: [0x44; 20],
            aave_borrow_amount: 6_200_000_000_000_000_000u128,
            uniswap_pool: [0xDD; 20],
            uniswap_token_in: [0x44; 20],
            uniswap_token_out: [0x55; 20],
            uniswap_amount_in: 6_200_000_000_000_000_000u128,
        },
        85,
        500,
        salt_with(7, 3),
    );
    println!("CROSS_PROTO_HEDGE_COMMITMENT=0x{}", hex::encode(i.commitment));

    // 0xFF — KillSwitch
    let i = AgentIntent::new_with_commitment(
        AgentId([0xA4; 20]),
        3,
        "Uniswap".into(),
        Action::KillSwitch {
            reason: "swarm_IL_exceeds_2.5%_threshold".into(),
        },
        100,
        0,
        salt_with(5, 3),
    );
    println!("KILLSWITCH_COMMITMENT=0x{}", hex::encode(i.commitment));
}
