# Findings

Everything below was read out of the token-2022 source that the tests actually
run against, not from documentation. Versions in play:

- **Runtime:** `spl-token-2022 11.0.0` — the program LiteSVM bundles
  (`litesvm-0.16.0/src/programs/elf/spl_token_2022-11.0.0.so`), i.e. the same
  code path mainnet runs today.
- **Compile-time interface:** `spl-token-2022-interface 2.1.0`, which is what
  `anchor-spl 1.2.0` re-exports as `anchor_spl::token_2022::spl_token_2022`.
- **Proof program:** `solana-zk-elgamal-proof-program 4.2.x`, registered as a
  builtin by LiteSVM's `solana-builtins`.

Line numbers are from the `spl-token-2022 11.0.0` and interface sources as read
while building this program.

## 1. The extension-set gap is enforced by token-2022, not by us

`ExtensionType::check_for_invalid_mint_extension_combinations`
(interface `extension/mod.rs`, ~1326-1374) rejects a mint that has
`TransferFeeConfig` **and** `ConfidentialTransferMint` without
`ConfidentialTransferFeeConfig`:

```rust
if transfer_fee_config && confidential_transfer_mint && !confidential_transfer_fee_config {
    return Err(TokenError::InvalidExtensionCombination);
}
```

and symmetrically rejects `ConfidentialTransferFeeConfig` unless both of the
others are present. The check runs inside `InitializeMint`
(processor.rs:125), right after the account length is validated, so the
combination can never be created — there is no "add it later" path either,
because mint extensions are only accepted while the mint is uninitialized.

*Reproduced by* `token_2022_refuses_transfer_fee_plus_confidential_transfers`,
which builds the mint with raw token-2022 instructions and asserts the failure
(`invalid combination of extensions`).

## 2. `InitializeMint` requires the account length to be exact

processor.rs:120-125:

```rust
let mut mint = PodStateWithExtensionsMut::<PodMint>::unpack_uninitialized(&mut mint_data)?;
let extension_types = mint.get_extension_types()?;
if ExtensionType::try_calculate_account_len::<Mint>(&extension_types)? != mint_data_len {
    return Err(ProgramError::InvalidAccountData);
}
```

Consequences that shaped `initialize_mint` and `reissue_mint`:

- The mint must be created with **exactly**
  `ExtensionType::try_calculate_account_len::<Mint>(&extensions)` bytes for the
  extension set initialized before `InitializeMint`. Over-allocating fails.
- The variable-length `TokenMetadata` entry therefore cannot be part of that
  length at all — `ExtensionType::try_get_type_len` errors for unsized types
  (`TokenMetadata => false` in `ExtensionType::sized`), so
  `try_calculate_account_len` cannot even express it.
- Instead the mint is funded for the *final* size and token-2022 grows the
  account when the metadata is written (`alloc_and_serialize_variable_len_extension`
  → `AccountInfo::resize`, which needs rent-exempt lamports for the new size).
  Hence `mint_account_space` vs `mint_rent_lamports` in `extensions.rs`.

Token **accounts** are the opposite: `_process_initialize_account`
(processor.rs:209-218) only requires
`try_calculate_account_len::<Account>(&required_extensions) <= data_len`, so
reserving extra space at creation is legal — which is how
`create_token_account` reserves room for `ConfidentialTransferAccount`.

## 3. A fee-bearing mint cannot use a plain confidential transfer

`confidential_transfer/processor.rs:655-780`: the confidential `Transfer`
instruction branches on the mint's extension set.

- No `TransferFeeConfig` → the plain path, proven by equality + ciphertext
  validity + range.
- With `TransferFeeConfig` → the `TransferWithFee` path, which *additionally*
  requires a fee sigma proof and a fee ciphertext validity proof, and checks
  them against
  `ConfidentialTransferFeeConfig.withdraw_withheld_authority_elgamal_pubkey`.

So keeping the fee does not merely add a deduction: it changes the proof set a
client has to produce, and the fee is withheld inside the destination account as
`ConfidentialTransferFeeAmount.withheld_amount` rather than being credited at
transfer time — issuer revenue stops being automatic and becomes a
proof-carrying `WithdrawWithheldTokens` operation.

That is why the re-issue drops `TransferFeeConfig`, and why
`confidential_transfer` / `withdraw_confidential` refuse a mint that has it
(`ConfidentialTransferUnavailableWithTransferFee`).

## 4. Seizure does not reach confidential balances, and frozen accounts win over the delegate

- `PermanentDelegate` moves tokens because the mint's delegate is accepted as
  an authority in `process_transfer` (processor.rs:455-462) — but
  `validate_owner` (processor.rs:2250-2292) still requires that authority to
  **sign** unless it is a multisig. The delegate is not a magic signature-free
  key.
- `if source_account.base.is_frozen() { return Err(TokenError::AccountFrozen) }`
  (processor.rs:356-358) runs for **every** transfer, before any delegate
  handling. A frozen account cannot be drained, not even by the permanent
  delegate: the flow is thaw → seize → re-freeze.
- Confidential balances are out of reach entirely: every confidential
  instruction takes the account *owner* as authority and needs the owner's
  ElGamal key pair (for the proofs) and AES key (for the new decryptable
  balance). A permanent delegate can confiscate the public balance, not the
  confidential one.

*Reproduced by* `seize_moves_funds_without_the_owner_and_refuses_frozen_accounts`.

## 5. `approve_policy = manual` is recorded by token-2022 but not enforced by it

- `ConfidentialTransferAccount.approved` is written at `ConfigureAccount` from
  the mint's `auto_approve_new_accounts` (confidential_transfer/processor.rs:289)
  and set to true by `ApproveAccount` (processor.rs:344).
- A search for reads of that field across the whole v11 source returns **only
  those two writes**. No deposit, transfer, apply or withdraw checks it.

Manual approval is therefore a policy the integrator must enforce. This program
does, in `confidential_ready` / `confidential_approved`: deposits, transfers,
pending-balance applications and withdrawals all require an approved account, and
a confidential transfer also requires an approved *destination*.

## 6. Confidential accounts need space that the ATA program will not give them

`ExtensionType::required_init_account_extensions` maps `TransferFeeConfig` to
`TransferFeeAmount` and maps `ConfidentialTransferMint` to **nothing**, so
`InitializeAccount` neither allocates nor initializes a confidential extension.
`ConfigureAccount` inserts it into existing space, and the processor notes that
"the caller is expected to use the `Reallocate` instruction to ensure there is
sufficient room" (confidential_transfer/processor.rs:283-287).

Hence the split the task asks about: opening the account is permissionless
(anyone may pay, exactly like the associated token account program — and an ATA
would *not* have the room), while configuring it is owner-only, owner-signed and
proof-carrying.

## 7. On-mint metadata grows the mint

`token_metadata/processor.rs:44-104`: `Initialize` requires
`metadata_info.key == mint_info.key` (the pointer points at the mint itself) and
the mint's mint authority to sign, then serializes the metadata into a new TLV
entry, growing the account. Reading it back is
`StateWithExtensions::get_variable_len_extension::<TokenMetadata>()`, which is
how `reissue_mint` carries name/symbol/uri across without trusting the caller.

## 8. Closing a mint

`process_close_account` (processor.rs:1356-1370) takes the mint branch only if
the account is not a token account, requires a `MintCloseAuthority`, and refuses
`supply != 0` (`MintHasSupply`). Decommissioning is therefore: burn the supply,
then close — and the program's own check mirrors that before the CPI.

## 9. Interface 2.x vs 3.x is a rename, not a re-encoding

`anchor-spl 1.2.0` re-exports `spl-token-2022-interface 2.1.0`, while token-2022
11.0.0 itself is built against the 3.x line. Diffing the two interfaces for
every module this program touches shows the confidential-transfer instruction
enum variants identical in name and order, and the only struct differences are
Rust-level renames of the same POD layouts:

| 2.x | 3.x |
| --- | --- |
| `spl_pod::optional_keys::OptionalNonZeroPubkey` | `solana_nullable::MaybeNull<Address>` |
| `spl_pod::primitives::PodBool` / `PodU64` | `solana_zero_copy::unaligned::Bool` / `U64` |
| `solana_zk_sdk::encryption::pod::elgamal::PodElGamalCiphertext` | `solana_zk_sdk_pod::encryption::elgamal::PodElGamalCiphertext` |

The bytes on the wire and in accounts are the same, so the program compiles
against 2.1.0 (via anchor-spl) and runs against the 11.0.0 program. The one
concrete effect is a type-alias level one: `ProofLocation` has to come from the
`0.5.x` line of `spl-token-confidential-transfer-proof-extraction` to match the
interface anchor-spl re-exports (`0.6.x` is a *different type* and does not
type-check against the 2.x builders).

## 10. How proofs reach a program that CPIs confidential instructions

`ProofLocation::InstructionOffset` means "the proof instruction sits at offset
*N* in the instruction stack", and the token-2022 program reads it through the
instructions sysvar. For a program wrapping the CPI that only works with a
**negative** offset (proof invoked before the token instruction), and the
transfer's three proofs do not fit in one transaction's 1232 bytes anyway.

The context state route used here is the practical one:

- the client creates the context account with `owner = ZK ElGamal proof
  program`, zeroed data and length exactly `size_of::<ProofContextState<U>>()`;
- the proof program verifies the proof and writes the context into it
  (it rejects a wrong owner, a wrong length, or an already-initialized
  `proof_type`);
- the instruction then references it with
  `ProofLocation::ContextStateAccount`.

One surprising detail from that code path: the proof program stores the
`context_state_authority` but never checks that it signed, so the security of the
account rests on the proof program owning it and on the account being
pre-created by the caller.
