# MERCURY — getting started



Rust stable/Cargo plus native TLS build prerequisites are needed (macOS: Xcode command-line tools; Linux: a C/C++ compiler, pkg-config and OpenSSL development headers).

```bash
git clone https://github.com/yMScorpion/MERCURY.git
cd MERCURY
# Offline engineering checks; no credentials, feeds or orders are needed:
cargo test --lib --locked
cargo build --locked
```

A separate manual research mode exists:

```bash
cargo run --release -- --config config/dry_run.yaml --dry-run
```

**This is not an offline simulator.** The binary performs external preflight checks and connects to configured feeds; enabled Telegram and signing/authentication paths may require configuration. Inspect [configuration](../config/dry_run.yaml) and [Getting started](GETTING_STARTED.md) before using it. Do not add real secrets to tracked files. `--dry-run` is an execution setting, not a blanket guarantee that all external side effects are disabled.


## Reproduction record

Use [VERIFICATION.md](VERIFICATION.md) to compare the code revision, toolchain and command. Never store real credentials in tracked files.
