# solana-program

HealthTrust smart contracts for the Solana Virtual Machine.

## prerequisites

- [Rust](https://rustup.rs/) (see `rust-toolchain.toml` for the pinned toolchain)
- [Solana CLI](https://docs.solana.com/cli/install-solana-cli-tools)
- [Anchor](https://www.anchor-lang.com/docs/installation) matching the workspace (`anchor-cli` 0.32.x aligns with `anchor-lang` in `Cargo.toml`)
- Node.js 20+ and [Yarn](https://yarnpkg.com/)

## quick start

```bash
git clone https://github.com/HealthTrust/solana-program.git
cd solana-program
yarn install
anchor build
anchor test
```

`anchor test` builds the program, starts a local validator, deploys, and runs the TypeScript tests under `tests/`.

## layout

| path | purpose |
|------|---------|
| `programs/healthtrust/` | on-chain program (Rust / Anchor) |
| `tests/` | integration tests against a local cluster |
| `migrations/` | deploy hooks used by the Anchor CLI |

## ci

Pull requests and pushes to `main` run `anchor build` and `anchor test` in GitHub Actions (`.github/workflows/ci.yml`).
