//! Print the three sub-program verifying keys (solver / execution / shapley)
//! in the on-chain `bytes32` shape — the values to feed into
//! `PrismHook.setSubVkeys(bytes32, bytes32, bytes32)` after deploy so the
//! Plan-B `settleEpochThreeProof` path is unlocked.
//!
//! Run after `cargo prove build` has produced all three sub-program ELFs:
//!
//! ```bash
//! cargo run --release --no-default-features \
//!     --features real-prover \
//!     -p prism-orchestrator \
//!     --example extract_sub_vkeys
//! ```

#[cfg(all(feature = "real-prover", not(feature = "mock-elf")))]
const SOLVER_ELF: &[u8] = include_bytes!(
    "../../../sp1-programs/solver-proof/elf/riscv32im-succinct-zkvm-elf"
);
#[cfg(all(feature = "real-prover", not(feature = "mock-elf")))]
const EXECUTION_ELF: &[u8] = include_bytes!(
    "../../../sp1-programs/execution-proof/elf/riscv32im-succinct-zkvm-elf"
);
#[cfg(all(feature = "real-prover", not(feature = "mock-elf")))]
const SHAPLEY_ELF: &[u8] = include_bytes!(
    "../../../sp1-programs/shapley-proof/elf/riscv32im-succinct-zkvm-elf"
);

#[cfg(all(feature = "real-prover", not(feature = "mock-elf")))]
fn main() -> anyhow::Result<()> {
    use sp1_sdk::{HashableKey, ProverClient};

    for (name, elf) in [
        ("solver", SOLVER_ELF),
        ("execution", EXECUTION_ELF),
        ("shapley", SHAPLEY_ELF),
    ] {
        if elf.is_empty() {
            anyhow::bail!("{}: ELF is empty — run `cargo prove build` first", name);
        }
    }

    let client = ProverClient::new();
    let (_pk, solver_vk) = client.setup(SOLVER_ELF);
    let (_pk, execution_vk) = client.setup(EXECUTION_ELF);
    let (_pk, shapley_vk) = client.setup(SHAPLEY_ELF);

    println!("solver_vkey    (bytes32) = {}", solver_vk.bytes32());
    println!("execution_vkey (bytes32) = {}", execution_vk.bytes32());
    println!("shapley_vkey   (bytes32) = {}", shapley_vk.bytes32());
    println!();
    println!("# cast send <PRISM_HOOK> 'setSubVkeys(bytes32,bytes32,bytes32)' \\");
    println!(
        "#   {} \\\n#   {} \\\n#   {} \\\n#   --rpc-url $UNICHAIN_RPC_URL --private-key $PRIVATE_KEY",
        solver_vk.bytes32(),
        execution_vk.bytes32(),
        shapley_vk.bytes32(),
    );
    Ok(())
}

#[cfg(not(all(feature = "real-prover", not(feature = "mock-elf"))))]
fn main() {
    eprintln!(
        "extract_sub_vkeys requires `--no-default-features --features real-prover`."
    );
    std::process::exit(1);
}
