import { publicKey, type PublicKey, type Umi } from "@metaplex-foundation/umi";
import { burn, fetchAsset as fetchAssetHelper } from "@metaplex-foundation/mpl-core";
import { base58 } from "@metaplex-foundation/umi/serializers";
import pc from "../color";
import { buildUmi, getExplorerUrl } from "./client";
import { fetchAssetsForOwner } from "./nft_fetch";

export async function burnNft(umi: Umi, asset: PublicKey | string): Promise<string> {
  const pk = publicKey(asset as string);
  const assetAccount = await fetchAssetHelper(umi, pk);
  const result = await (burn as any)(umi, { asset: assetAccount }).sendAndConfirm(umi);
  const signature = base58.deserialize(result.signature)[0];
  console.log(pc.red(`burned ${String(pk)}`) + pc.dim(" sig:") + pc.green(signature));
  console.log(pc.cyan(getExplorerUrl(signature)));
  return signature;
}

export async function burnAllForOwner(umi: Umi): Promise<string[]> {
  const assets = await fetchAssetsForOwner(umi);
  console.log(
    pc.yellow(`Found ${assets.length} assets for ${umi.identity.publicKey} — burning...`),
  );
  const sigs: string[] = [];
  for (const a of assets) {
    const sig = await burnNft(umi, a.publicKey);
    sigs.push(sig);
  }
  console.log(
    pc.green(`Burned ${sigs.length} assets.`) +
      pc.dim(
        " Arweave URIs (gateway.irys.xyz/...) remain permanently — only on-chain AssetV1 closed.",
      ),
  );
  return sigs;
}

// Standalone run: bun src/nft/nft_burn.ts [assetPubkey]
// no arg → burns all assets for devnet-wallet.json (use to start from scratch)
if (import.meta.main) {
  const umi = buildUmi();
  const asset = process.argv[2];
  if (asset) await burnNft(umi, asset);
  else await burnAllForOwner(umi);
}
