# PRISM build / verification targets.
#
# Most day-to-day work uses `cargo` directly. This Makefile only has
# wrappers for things that span multiple crates or shell out to scripts —
# specifically, ELF artifact verification and the SP1-program rebuild
# loop, both of which need to be done identically across machines or
# the AGGREGATOR_VKEY drifts silently.

SP1_PROGRAMS := solver-proof execution-proof shapley-proof aggregator
ELF_PATH = sp1-programs/$(1)/elf/riscv32im-succinct-zkvm-elf

# Pinned SP1 SDK build — bump alongside the toolchain in `sp1-programs/*/Cargo.toml`.
# `cargo prove --version` must contain this commit hash for `verify-sp1-version`
# to pass. A drift means ELF rebuilds may produce a different vkey, which
# silently breaks the deployed AGGREGATOR_VKEY mapping (see C2 in
# AUDIT_REPORT). Bumping the pin means: (1) reinstall SP1 toolchain, (2)
# `make rebuild-elfs && make refresh-elf-shas`, (3) re-extract + redeploy.
SP1_PINNED_COMMIT := 563ede1

.PHONY: help test verify-elf refresh-elf-shas rebuild-elfs extract-vkey verify-sp1-version

help:
	@echo "PRISM Makefile targets:"
	@echo "  make test               — cargo test --workspace"
	@echo "  make verify-elf         — assert ELF SHAs match ELF_SHAS.txt"
	@echo "  make refresh-elf-shas   — rewrite ELF_SHAS.txt from current ELFs"
	@echo "  make rebuild-elfs       — cargo prove build × 4 SP1 programs"
	@echo "  make extract-vkey       — print current AGGREGATOR_VKEY"
	@echo "  make verify-sp1-version — assert pinned SP1 toolchain (commit $(SP1_PINNED_COMMIT))"

test:
	cargo test --workspace

verify-elf:
	@fail=0; \
	for prog in $(SP1_PROGRAMS); do \
		expected=$$(grep "^$$prog:" ELF_SHAS.txt | awk '{print $$2}' | sed 's/sha256://'); \
		if [ -z "$$expected" ] || [ "$$expected" = "PENDING_REBUILD_THIS_COMMIT" ]; then \
			echo "SKIP $$prog (no recorded SHA)"; \
			continue; \
		fi; \
		actual=$$(sha256sum sp1-programs/$$prog/elf/riscv32im-succinct-zkvm-elf 2>/dev/null | awk '{print $$1}'); \
		if [ -z "$$actual" ]; then \
			echo "MISS $$prog (ELF not built)"; fail=1; continue; \
		fi; \
		if [ "$$expected" != "$$actual" ]; then \
			echo "FAIL $$prog: expected $$expected, got $$actual"; fail=1; \
		else \
			echo "OK   $$prog: $$actual"; \
		fi; \
	done; \
	if [ "$$fail" = "1" ]; then exit 1; fi

refresh-elf-shas:
	@{ \
		echo "# SP1 ELF artifact SHAs — defense against silent vkey drift."; \
		echo "#"; \
		echo "# Each entry rotates when the corresponding \`sp1-programs/<name>/src/main.rs\`"; \
		echo "# changes. The aggregator's vkey (in AGGREGATOR_VKEY.txt) is keyed on its"; \
		echo "# own ELF only — sub-program vkeys come in via stdin at proof time, so a"; \
		echo "# sub-program rotation does NOT force an aggregator redeploy."; \
		echo "#"; \
		echo "# Verify locally with: \`make verify-elf\`"; \
		echo "# Bump after rebuild with: \`make refresh-elf-shas\`"; \
		echo ""; \
		for prog in $(SP1_PROGRAMS); do \
			hash=$$(sha256sum sp1-programs/$$prog/elf/riscv32im-succinct-zkvm-elf 2>/dev/null | awk '{print $$1}'); \
			if [ -z "$$hash" ]; then hash="MISSING"; fi; \
			printf "%-16s sha256:%s\n" "$$prog:" "$$hash"; \
		done; \
	} > ELF_SHAS.txt
	@echo "wrote ELF_SHAS.txt"

rebuild-elfs:
	@for prog in $(SP1_PROGRAMS); do \
		echo ">>> cargo prove build sp1-programs/$$prog"; \
		(cd sp1-programs/$$prog && cargo prove build) || exit 1; \
		cp sp1-programs/$$prog/target/elf-compilation/riscv32im-succinct-zkvm-elf/release/$$prog \
			sp1-programs/$$prog/elf/riscv32im-succinct-zkvm-elf; \
	done
	@echo "rebuilt all 4 ELFs — run \`make refresh-elf-shas\` and \`make extract-vkey\` next"

extract-vkey:
	cargo run --release --no-default-features --features real-prover \
		-p prism-orchestrator --example extract_aggregator_vkey

# Build hygiene: assert the installed SP1 toolchain matches the pinned commit.
# `cargo prove --version` prints e.g. `cargo-prove sp1 (563ede1 ...)`.
verify-sp1-version:
	@actual=$$(cargo prove --version 2>/dev/null | head -n1); \
	if [ -z "$$actual" ]; then \
		echo "FAIL: cargo prove not installed — run \`sp1up\` first"; exit 1; \
	fi; \
	echo "expected: cargo-prove with commit $(SP1_PINNED_COMMIT)"; \
	echo "actual:   $$actual"; \
	if echo "$$actual" | grep -q "$(SP1_PINNED_COMMIT)"; then \
		echo "OK"; \
	else \
		echo "FAIL: SP1 toolchain commit drifted — pin is $(SP1_PINNED_COMMIT)."; \
		echo "      Reinstall with \`sp1up --version <pinned>\` then \`make rebuild-elfs\`."; \
		exit 1; \
	fi
