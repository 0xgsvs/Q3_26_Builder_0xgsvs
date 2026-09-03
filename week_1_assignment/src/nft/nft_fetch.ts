import { publicKey, type PublicKey, type Umi } from "@metaplex-foundation/umi";
import { fetchAsset as fetchAssetHelper, fetchAssetsByOwner } from "@metaplex-foundation/mpl-core";
import pc from "../color";
import { buildUmi } from "./client";

export async function fetchAsset(
  umi: Umi,
  asset: PublicKey | string,
  opts?: { retries?: number; delayMs?: number },
) {
  const pk = publicKey(asset as string);
  const retries = opts?.retries ?? 12;
  const delayMs = opts?.delayMs ?? 800;
  let lastErr: unknown;
  for (let i = 0; i < retries; i++) {
    try {
      return await fetchAssetHelper(umi, pk);
    } catch (e: any) {
      lastErr = e;
      const msg = String(e?.message ?? e);
      // Only retry on not-found / account missing, not on logic errors
      if (
        !msg.includes("was not found") &&
        !msg.includes("AccountNotFound") &&
        !msg.includes("not found")
      )
        throw e;
      if (i === retries - 1) throw e;
      await new Promise((r) => setTimeout(r, delayMs));
    }
  }
  throw lastErr;
}

export async function fetchAssetsForOwner(umi: Umi, owner?: PublicKey | string) {
  const ownerPk = owner ? publicKey(owner as string) : umi.identity.publicKey;
  return fetchAssetsByOwner(umi, ownerPk);
}

// Standalone run: bun src/nft/nft_fetch.ts <assetPubkey>
if (import.meta.main) {
  const umi = buildUmi();
  const asset = process.argv[2];
  if (asset) {
    const fetched = await fetchAsset(umi, asset);
    console.log(JSON.stringify(fetched, null, 2));
  } else {
    const assets = await fetchAssetsForOwner(umi);
    console.log(pc.yellow(`Found ${assets.length} assets for ${umi.identity.publicKey}`));
    for (const a of assets)
      console.log(
        pc.green(`- ${a.publicKey}`) + pc.dim(` | ${a.name} | ${a.uri} | owner:${a.owner}`),
      );
  }
}
