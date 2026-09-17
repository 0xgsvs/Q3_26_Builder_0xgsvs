# AMM

Constant-product AMM (CPMM) on Anchor. One pool per `seed`: users deposit X/Y for LP
tokens, swap X<->Y against the constant-product curve, and burn LP to withdraw.
Swaps charge a protocol fee in basis points that is routed to a treasury wallet.
An optional authority can update the fee or lock the pool.

Program ID (localnet): `4athVTb9MzXtknSnuywhYJAAudHV2SfMqZTVuVqgDWJ1`

Reference: `amm_q2_26/programs/amm-video` (Dean Little's Anchor AMM), ported to
`anchor-lang`/`anchor-spl 1.2.0` + `litesvm 0.16` and extended with fees + treasury.

## PDAs

- `config`: `seeds = ["config", seed.to_le_bytes()]`, stores `seed, authority,
  treasury, mint_x, mint_y, fee, locked, bumps`.
- `mint_lp`: `seeds = ["lp", config]`, LP mint (6 decimals, authority = `config`).
- `vault_x` / `vault_y`: ATAs of `mint_x`/`mint_y` owned by `config`, hold reserves.
- `treasury_x` / `treasury_y`: ATAs of `mint_x`/`mint_y` owned by `treasury`
  (`init_if_needed` on swap), collect the protocol fee.
- `user_lp`: ATA of `mint_lp` owned by the user (`init_if_needed` on deposit).

## Instructions

- `initialize(seed, fee, authority)`: creates `config`, `mint_lp`, `vault_x`,
  `vault_y`. Rejects `fee > 10_000` bps and `mint_x == mint_y`. `treasury` is a
  separate unchecked account (any wallet; only its pubkey is stored).
- `deposit(amount, max_x, max_y)`: first deposit seeds the pool at any ratio
  (`max_x`/`max_y`); later deposits are proportional via
  `ConstantProduct::xy_deposit_amounts_from_l`, reverting on slippage. Mints
  `amount` LP to `user_lp`.
- `withdraw(amount, min_x, min_y)`: burns `amount` LP, returns proportional X/Y
  via `xy_withdraw_amounts_from_l`, reverting on slippage.
- `swap(is_x, amount_in, min_amount_out)`: takes `fee` bps off the top into
  `treasury_x`/`treasury_y`, quotes the net amount on the curve with zero curve
  fee (so the fee is charged exactly once), and pays out the other side.
  Reverts on slippage or treasury mismatch.
- `update(fee, locked)`: authority-only (requires `config.authority == signer`,
  rejects unset authority and `fee > 10_000`). Used to change the fee or
  lock/unlock the pool. `deposit`/`withdraw`/`swap` all reject a locked pool.

## Fees and treasury

- `fee` is stored in basis points (`FEE_DENOMINATOR = 10_000`).
- On swap: `fee_amount = amount_in * fee / 10_000` goes `user -> treasury` ATA;
  `net = amount_in - fee_amount` goes `user -> vault` and is the curve input.
  LP holders are not diluted: the fee never touches reserves or LP supply.
- `treasury` is set once at `initialize` and is checked (`treasury.key() ==
  config.treasury`) on every swap.

## Errors

`FeePercentErr, PoolLocked, SlippageExceeded, Overflow, Underflow,
InvalidToken, InvalidTreasury, InvalidAuthority, NoAuthoritySet,
InvalidAmount, InvalidPrecision, InsufficientBalance, ZeroBalance,
CurveError, InvalidFee` (see `programs/amm/src/error.rs`).

## Layout

- `programs/amm/src/lib.rs` — entrypoints
- `constants.rs` — `CONFIG_SEED`, `LP_SEED`, `FEE_DENOMINATOR`, `CURVE_PRECISION`
- `state.rs` — `Config`
- `instructions/{initialize,deposit,withdraw,swap,update}.rs`
- `tests/test_amm.rs` — LiteSVM + litesvm-token tests (`Setup`, `send` helper
  with `expire_blockhash`, per-instruction ix builders)

## Test

```bash
anchor build
cargo nextest run
```

Tests (15): `initialize_ok`, `initialize_rejects_fee_above_10000`,
`initialize_rejects_same_mint`, `deposit_first_and_second`,
`deposit_slippage_fail`, `deposit_zero_amount_fail`, `withdraw_ok`,
`withdraw_slippage_fail`, `swap_x_for_y_routes_fee_to_treasury`,
`swap_y_for_x_routes_fee_to_treasury`, `swap_slippage_fail`,
`swap_wrong_treasury_fail`, `update_fee_and_lock`, `update_unauthorized_fail`,
`update_invalid_fee_fail`.
