# Escrow

Token-for-token escrow using Anchor PDAs + Token-2022-compatible `TokenInterface`. Maker deposits A, taker fills with B.

Program ID (localnet): `cVefNVDPPmedcognHyK1NQMz1iUJjEmVi4rcHB4AYWu`

## PDAs

- `escrow`: `seeds = ["escrow", maker, seed.to_le_bytes()]`, stores `seed, maker, mint_a, mint_b, receive, bump, expiration`.
- `vault`: ATA `mint_a` with authority `escrow`, holds deposited A.

## Instructions

- `make(seed, deposit, receive, expiration)`: init `escrow`, init vault ATA, `transfer_checked` maker A -> vault.
- `take`: taker B -> maker (`receive`), vault A -> taker, close vault to maker, close `escrow` to maker.
- `refund`: vault A -> maker, close vault to maker, close `escrow` to maker.
- `update(receive, expiration)`: maker-only (`has_one = maker` + seeds), sets `receive` / `expiration`.

## Layout

- `programs/escrow/src/lib.rs` — program entrypoints
- `constants.rs` — `ESCROW_SEED`
- `state.rs` — `Escrow`
- `instructions/{make,take,refund,update}.rs`
- `tests/test_escrow.rs` — LiteSVM + litesvm-token tests with `Setup::{send_as_maker,send_as_taker}` helper

## Test

```bash
anchor test --skip-build
# or: cargo nextest run
```

Tests: `escrow_make`, `escrow_update`, `escrow_take`, `escrow_refund`. Setup creates `mint_a/mint_b`, funds `maker_ata_a` / `taker_ata_b`, derives `escrow` + vault ATA.
