# Vault

Per-user SOL vault using Anchor PDAs. One `vault_state` + one `vault` (SystemAccount holding lamports) per user.

Program ID (localnet): `J24rwama28rwAbd8qr9Vgm36c13TjGXmWHZNqPTHkGX6`

## PDAs

- `vault_state`: `seeds = ["vault_state", user]`, stores `vault_bump`, `state_bump`.
- `vault`: `seeds = ["vault", vault_state]`, `SystemAccount`, holds SOL. Derived after `vault_state`.

## Instructions

- `initialize`: init `vault_state` (payer = user), validate `vault` PDA, store bumps.
- `deposit { amount }`: system transfer `user -> vault`.
- `withdraw { amount }`: system transfer `vault -> user` signed by vault PDA (`["vault", vault_state, vault_bump]`).
- `close`: drain `vault` to `user`, `close = user` on `vault_state`.

## Layout

- `programs/vault/src/lib.rs` — program entrypoints
- `constants.rs` — `VAULT_SEED`, `VAULT_STATE_SEED`
- `state.rs` — `VaultState`
- `instructions/{initialize,deposit,withdraw,close}.rs`
- `tests/test_vault.rs` — LiteSVM tests with `Setup::send_transaction` helper

## Test

```bash
anchor test --skip-build
# or: cargo nextest run
```

Tests: `vault_initialize`, `vault_deposit`, `vault_withdraw`, `vault_close`. Chained-PDA note: `vault` depends on `vault_state`, so derive `vault_state` first.
