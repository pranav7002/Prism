//! Deterministic intent generator that walks the pivot's three-epoch demo.
//!
//! Epochs cycle through `Calm → Opportunity → Crisis` keyed off `epoch % 3`,
//! exercising every action variant in `prism_types::Action`. Salts are
//! derived from the agent role + epoch so test runs are reproducible.
//!
//! - **Calm (`epoch % 3 == 1`)**: α AddLiquidity, β SetDynamicFee,
//!   γ small BatchConsolidate, δ low-priority Backrun, ε DeltaHedge monitor.
//! - **Opportunity (`epoch % 3 == 2`)**: α RemoveLiquidity + AddLiquidity at
//!   new range (two intents, separate agents), β MigrateLiquidity
//!   0.30% → 0.60%, γ BatchConsolidate, δ high-priority Backrun, ε monitor.
//! - **Crisis (`epoch % 3 == 0`)**: ε CrossProtocolHedge + KillSwitch;
//!   the kill-switch suppresses everyone else at solver time. We still emit
//!   the other agents' intents so the WebSocket stream visibly shows what
//!   got dropped.

use prism_types::{Action, AgentId, AgentIntent, ConsolidateAdd, ConsolidateRemove};

const PREDICTIVE_LP: [u8; 20] = [0xA0; 20];
const FEE_CURATOR: [u8; 20] = [0xA1; 20];
const FRAG_HEALER: [u8; 20] = [0xA2; 20];
const BACKRUNNER: [u8; 20] = [0xA3; 20];
const GUARDIAN: [u8; 20] = [0xA4; 20];

const POOL_USDC_WETH_005: [u8; 20] = [
    0x88, 0xe6, 0xa0, 0xc2, 0xdd, 0xd2, 0x6f, 0xee, 0xb6, 0x4f, 0x03, 0x9a, 0x2c, 0x41, 0x29, 0x6f,
    0xcb, 0x3f, 0x56, 0x40,
];
const POOL_USDC_WETH_030: [u8; 20] = [
    0x8a, 0xd5, 0x99, 0xc3, 0xa0, 0xff, 0x1d, 0xe0, 0x82, 0x01, 0x1e, 0xfd, 0xdc, 0x58, 0xf1, 0x90,
    0x8e, 0xb6, 0xe6, 0xd8,
];
const POOL_USDC_WETH_060: [u8; 20] = [
    0x7b, 0xea, 0x39, 0x86, 0x7e, 0x42, 0x66, 0x81, 0xf6, 0xa1, 0x12, 0x7c, 0xff, 0x9e, 0x65, 0xbf,
    0x63, 0x8f, 0xb2, 0x9e,
];

const TOKEN_USDC: [u8; 20] = [
    0xa0, 0xb8, 0x69, 0x91, 0xc6, 0x21, 0x8b, 0x36, 0xc1, 0xd1, 0x9d, 0x4a, 0x2e, 0x9e, 0xb0, 0xce,
    0x36, 0x06, 0xeb, 0x48,
];
const TOKEN_WETH: [u8; 20] = [
    0xc0, 0x2a, 0xaa, 0x39, 0xb2, 0x23, 0xfe, 0x8d, 0x0a, 0x0e, 0x5c, 0x4f, 0x27, 0xea, 0xd9, 0x08,
    0x3c, 0x75, 0x6c, 0xc2,
];

fn salt_for(role: u8, epoch: u64) -> [u8; 32] {
    let mut s = [0u8; 32];
    s[0] = role;
    s[24..32].copy_from_slice(&epoch.to_be_bytes());
    s
}

fn mk_intent(
    role: u8,
    agent: [u8; 20],
    epoch: u64,
    action: Action,
    priority: u8,
    max_slippage_bps: u16,
) -> AgentIntent {
    AgentIntent::new_with_commitment(
        AgentId(agent),
        epoch,
        "Uniswap".into(),
        action,
        priority,
        max_slippage_bps,
        salt_for(role, epoch),
    )
}

/// Label of the current demo scenario for the given epoch.
pub fn scenario_for(epoch: u64) -> &'static str {
    match epoch % 3 {
        1 => "calm",
        2 => "opportunity",
        _ => "crisis",
    }
}

/// Produce five deterministic intents matching the pivot demo scenario for
/// this epoch. Intents are always one-per-agent (α, β, γ, δ, ε).
pub fn generate_mock_intents(epoch: u64) -> Vec<AgentIntent> {
    match epoch % 3 {
        1 => calm_intents(epoch),
        2 => opportunity_intents(epoch),
        _ => crisis_intents(epoch),
    }
}

fn calm_intents(epoch: u64) -> Vec<AgentIntent> {
    vec![
        // α Predictive LP — tight range, 0.30% fee tier.
        mk_intent(
            0,
            PREDICTIVE_LP,
            epoch,
            Action::AddLiquidity {
                pool: POOL_USDC_WETH_030,
                amount0: 300_000_000_000u128, // 300k USDC
                amount1: 100_000_000_000_000_000_000u128, // 100 WETH
                tick_lower: 196_800,
                tick_upper: 200_400,
            },
            70,
            50,
        ),
        // β Fee Curator — set dynamic fee to 0.30% (3000 ppm).
        mk_intent(
            1,
            FEE_CURATOR,
            epoch,
            Action::SetDynamicFee {
                pool: POOL_USDC_WETH_030,
                new_fee_ppm: 3_000,
            },
            65,
            0,
        ),
        // γ Frag Healer — consolidate one stale 0.05% position into 0.30%.
        mk_intent(
            2,
            FRAG_HEALER,
            epoch,
            Action::BatchConsolidate {
                removes: vec![ConsolidateRemove {
                    pool: POOL_USDC_WETH_005,
                    liquidity: 45_000_000_000u128,
                }],
                adds: vec![ConsolidateAdd {
                    pool: POOL_USDC_WETH_030,
                    amount0: 45_000_000_000u128,
                    amount1: 15_000_000_000_000_000_000u128,
                    tick_lower: 196_800,
                    tick_upper: 200_400,
                }],
            },
            50,
            50,
        ),
        // δ Backrunner — low-priority idle backrun.
        mk_intent(
            3,
            BACKRUNNER,
            epoch,
            Action::Backrun {
                target_tx: {
                    let mut t = [0u8; 32];
                    t[0] = 0xBE;
                    t[24..32].copy_from_slice(&epoch.to_be_bytes());
                    t
                },
                profit_token: TOKEN_USDC,
            },
            50,
            100,
        ),
        // ε Guardian — light inventory hedge.
        mk_intent(
            4,
            GUARDIAN,
            epoch,
            Action::DeltaHedge {
                position_id: 1,
                delta: -100,
            },
            40,
            50,
        ),
    ]
}

fn opportunity_intents(epoch: u64) -> Vec<AgentIntent> {
    vec![
        // α Predictive LP — reposition upper (exit old range, add new).
        mk_intent(
            0,
            PREDICTIVE_LP,
            epoch,
            Action::AddLiquidity {
                pool: POOL_USDC_WETH_030,
                amount0: 300_000_000_000u128,
                amount1: 100_000_000_000_000_000_000u128,
                tick_lower: 200_400,
                tick_upper: 203_400,
            },
            85,
            50,
        ),
        // β Fee Curator — migrate to 0.60% on volatility spike.
        mk_intent(
            1,
            FEE_CURATOR,
            epoch,
            Action::MigrateLiquidity {
                from_pool: POOL_USDC_WETH_030,
                to_pool: POOL_USDC_WETH_060,
                amount: 200_000_000_000u128,
                tick_lower: 200_400,
                tick_upper: 203_400,
            },
            75,
            75,
        ),
        // γ Frag Healer — sweep three stale positions into active pool.
        mk_intent(
            2,
            FRAG_HEALER,
            epoch,
            Action::BatchConsolidate {
                removes: vec![
                    ConsolidateRemove {
                        pool: POOL_USDC_WETH_005,
                        liquidity: 80_000_000_000u128,
                    },
                    ConsolidateRemove {
                        pool: POOL_USDC_WETH_030,
                        liquidity: 45_000_000_000u128,
                    },
                    ConsolidateRemove {
                        pool: POOL_USDC_WETH_060,
                        liquidity: 60_000_000_000u128,
                    },
                ],
                adds: vec![ConsolidateAdd {
                    pool: POOL_USDC_WETH_030,
                    amount0: 150_000_000_000u128,
                    amount1: 50_000_000_000_000_000_000u128,
                    tick_lower: 200_400,
                    tick_upper: 203_400,
                }],
            },
            55,
            75,
        ),
        // δ Backrunner — exploit β's thinned 0.30% pool, target-tx binds to β.
        mk_intent(
            3,
            BACKRUNNER,
            epoch,
            Action::Backrun {
                target_tx: {
                    let mut t = [0u8; 32];
                    t[0] = 0xBE;
                    t[1] = 0xEF;
                    t[24..32].copy_from_slice(&epoch.to_be_bytes());
                    t
                },
                profit_token: TOKEN_USDC,
            },
            90,
            100,
        ),
        // ε Guardian — inventory hedge; no crisis yet.
        mk_intent(
            4,
            GUARDIAN,
            epoch,
            Action::DeltaHedge {
                position_id: 1,
                delta: -500,
            },
            40,
            50,
        ),
    ]
}
