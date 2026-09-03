# Week 1 Assignment — SPL + MPL Core NFT

Two tasks in one repo: **SPL token lifecycle** and **MPL Core NFT lifecycle** (Umi + Irys). Both use zero-dep colorized terminal logs.

## Why localhost for SPL and devnet for NFT

**a. SPL on localhost** — I was testing many things and hit devnet airdrop rate limits (`429`, faucet drains, slow finality). So I opted for **localhost via Surfpool validator (`solanaLocalRpc()` / `http://127.0.0.1:8899`) for direct deployment** and **`litesvm` for testing** (`tests/spl.test.ts:39` `buildSplClient(litesvm())`). Instant, deterministic, no airdrop constraints, cheaper iteration.

**b. NFT on devnet** — For NFTs we interact with **Irys (Arweave) and Metaplex Core services** to upload image + JSON and mint on-chain. Those services are wired to **devnet** (`https://devnet.irys.xyz` → `https://gateway.irys.xyz/...`). So I use **devnet all the way** here: `src/nft/client.ts:19` `buildUmi("https://api.devnet.solana.com")` + funded `devnet-wallet.json`, reused `IMAGE_URI`/`METADATA_URI`. Arweave is permanent and gateway is chain-agnostic — burning the `AssetV1` later doesn't delete the image/metadata.

---

## Tasks

### 1) SPL Token — `src/spl.ts`

Init a mint, mint to payer ATA, transfer to recipient — all via `@solana/kit` + `@solana-program/token`.

- `buildSplClient(transport)` — generates ephemeral signer, `createClient().use(signer).use(transport).use(airdropSigner).use(tokenProgram())`. Default `solanaLocalRpc()`, tests pass `litesvm()`.
- `initMint(client)` — `token.instructions.createMint({decimals:9})` + derives ATA via `findAssociatedTokenPda`.
- `mintToPayer(client, mint)` — `mintToATA` `MINT_AMOUNT=2_000_000_000n`.
- `transferToRecipient(client, mint)` — `transferToATA` to `RECIPIENT=9aDCHG...` full amount.
- Colorized: `magenta` headers, `green` wallet/mint, `cyan` explorer `?cluster=custom`.

Run live (needs Surfpool `solana-test-validator`):

```bash
bun src/spl.ts
```

### 2) NFT (MPL Core + Umi + Irys) — `src/nft/`

Modular, one concern per file (no monolithic `nft_umi.ts`):

```
src/nft/
  client.ts       — buildUmi(), IMAGE_URI/METADATA_URI/NFT_NAME, getExplorerUrl()
  nft_image.ts    — uploadImage(umi, path) → gateway.irys.xyz/BS8y...
  nft_metadata.ts — uploadMetadata(umi, {image}) → gateway.irys.xyz/6ZeGF...
  nft_mint.ts     — mintNft(umi, {name,uri}) → create+sendAndConfirm (AssetV1)
  nft_update.ts   — updateNft(umi, asset, {newName,newUri}) → updateV1+some()
  nft_fetch.ts    — fetchAsset(umi, asset) with retry, fetchAssetsForOwner
  nft_burn.ts     — burnNft(umi, asset), burnAllForOwner() (closes AssetV1, Arweave stays)
```

- **Client** `src/nft/client.ts:18` — `createUmi(devnet).use(mplCore()).use(irysUploader({address:"https://devnet.irys.xyz"})).use(signerIdentity(createSignerFromKeypair(wallet)))`, no airdrop (wallet funded).
- **Reuse** — `IMAGE_URI="BS8yVgsk..."` and `METADATA_URI="6ZeGF2XY..."` are already on Arweave (permanent). Re-minting after `burn` reuses them — no re-upload cost/delay. New content → `uploadImage`/`uploadMetadata` once, commit new uri.
- **Logs** — `dim` labels, `green` sig/asset, `cyan` explorer (`?cluster=devnet` + `core.metaplex.com`), `yellow` updated fields, `red` burn.

Standalone:

```bash
bun src/nft/nft_image.ts      # upload pixelnft.png → BS8y...
bun src/nft/nft_metadata.ts   # upload JSON → 6ZeGF...
bun src/nft/nft_mint.ts       # mint Pixel_pic
bun src/nft/nft_fetch.ts              # list all for wallet
bun src/nft/nft_fetch.ts <asset>      # fetch one
bun src/nft/nft_update.ts <asset>     # update name/uri
bun src/nft/nft_burn.ts               # burn all (start from scratch)
bun src/nft/nft_burn.ts <asset>       # burn one
```

---

## Install & Test

```bash
bun install

# SPL — fast, local (litesvm, no network)
bun run test:spl
# → Init/Mint/Transfer with custom explorer links, asserts supply/balances

# NFT — real devnet (needs network + funded devnet-wallet.json)
bun run test:nft
# → mint (reuse 6ZeGF...) → fetch → update (→ poll 800ms×12 for finality) → fetch → burn → poll until not found
# If 429 rate-limited, retry is built in (see nft_fetch.ts:5 retries=12)

# All tests (Vitest)
bun run test
# 6 pass (3 SPL + 3 NFT)

# Typecheck
bun run typecheck

# Lint & Format
bun run lint
bun run format:check
```

## Notes

- **Arweave permanence** — `gateway.irys.xyz/...` is not deleted by `burn`; only the on-chain `AssetV1` (`CoREENxT...`) is closed. Keep hardcoding URIs unless you need new metadata.
- **Devnet finality** — `nft_fetch.ts:5` retries on `AccountNotFoundError` and tests poll after `updateV1`/`burn` to handle 1-3s propagation and `429 Too Many Requests` retries.
- **Colors** — via native ANSI color helper (`src/color.ts`, zero-dependency). Headers `magenta.bold`, explorer `cyan`, addresses `green`.
