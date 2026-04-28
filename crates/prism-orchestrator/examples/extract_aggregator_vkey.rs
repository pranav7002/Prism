//! Print the aggregator program's verifying key in the on-chain `bytes32`
//! shape — the value to embed as `AGGREGATOR_VKEY` in
//! `PrismCoordinator.sol`.
//!
//! Run after `cargo prove build` has produced
//! `sp1-programs/aggregator/elf/riscv32im-succinct-zkvm-elf`:
//!
//! ```bash
//! cargo run --release --no-default-features \
//!     --features real-prover \
//!     -p prism-orchestrator \
//!     --example extract_aggregator_vkey
//! ```
//!
//! Note: this example deliberately bypasses the orchestrator binary's
//! `mock-elf`-default feature setup by `include_bytes!`-ing the ELF
//! directly. Build with `--no-default-features --features real-prover` so
//! the include path resolves to the real artifact.

#[cfg(all(feature = "real-prover", not(feature = "mock-elf")))]
const AGGREGATOR_ELF: &[u8] = include_bytes!(
    "../../../sp1-programs/aggregator/elf/riscv32im-succinct-zkvm-elf"
);

#[cfg(all(feature = "real-prover", not(feature = "mock-elf")))]
fn main() -> anyhow::Result<()> {
    use sp1_sdk::{HashableKey, ProverClient};

    if AGGREGATOR_ELF.is_empty() {
        anyhow::bail!(
            "AGGREGATOR_ELF is empty — build the aggregator with `cargo prove build` first"
        );
    }

    let client = ProverClient::new();
    let (_pk, vk) = client.setup(AGGREGATOR_ELF);

    println!("AGGREGATOR_VKEY (bytes32) = {}", vk.bytes32());
    Ok(())
}

#[cfg(not(all(feature = "real-prover", not(feature = "mock-elf"))))]
fn main() {
    eprintln!(
        "extract_aggregator_vkey requires `--no-default-features --features real-prover`."
    );
    std::process::exit(1);
}
