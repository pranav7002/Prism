use prism_types::{Action, AgentId, AgentIntent};

fn main() {
    // Same as sample_intent() in lib.rs tests
    let intent = AgentIntent::new_with_commitment(
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
    println!("SWAP_COMMITMENT=0x{}", hex::encode(intent.commitment));

    // AddLiquidity vector
    let intent2 = AgentIntent::new_with_commitment(
        AgentId([0xA0; 20]),
        1,
        "Uniswap".into(),
        Action::AddLiquidity {
            pool: [0x8a, 0xd5, 0x99, 0xc3, 0xa0, 0xff, 0x1d, 0xe0, 0x82, 0x01,
                   0x1e, 0xfd, 0xdc, 0x58, 0xf1, 0x90, 0x8e, 0xb6, 0xe6, 0xd8],
            amount0: 300_000_000_000u128,
            amount1: 100_000_000_000_000_000_000u128,
            tick_lower: 196_800,
            tick_upper: 200_400,
        },
        70,
        50,
        {
            let mut s = [0u8; 32];
            s[0] = 0;
            s[24..32].copy_from_slice(&1u64.to_be_bytes());
            s
        },
    );
    println!("ADD_LIQ_COMMITMENT=0x{}", hex::encode(intent2.commitment));

    // KillSwitch vector
    let intent3 = AgentIntent::new_with_commitment(
        AgentId([0xA4; 20]),
        3,
        "Uniswap".into(),
        Action::KillSwitch {
            reason: "swarm_IL_exceeds_2.5%_threshold".into(),
        },
        100,
        0,
        {
            let mut s = [0u8; 32];
            s[0] = 5;
            s[24..32].copy_from_slice(&3u64.to_be_bytes());
            s
        },
    );
    println!("KILLSWITCH_COMMITMENT=0x{}", hex::encode(intent3.commitment));
}
