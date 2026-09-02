import { generateSigner, type Umi } from "@metaplex-foundation/umi";
import { create } from "@metaplex-foundation/mpl-core";
import { base58 } from "@metaplex-foundation/umi/serializers";
import pc from "picocolors";
import { buildUmi, getCoreExplorerUrl, getExplorerUrl, METADATA_URI, NFT_NAME } from "./client";

export async function mintNft(
  umi: Umi,
  opts?: { name?: string; uri?: string },
): Promise<{ asset: ReturnType<typeof generateSigner>; signature: string }> {
  const name = opts?.name ?? NFT_NAME;
  const uri = opts?.uri ?? METADATA_URI;
  const asset = generateSigner(umi);

  const tx = await create(umi, { uri, name, asset }).sendAndConfirm(umi);
  const signature = base58.deserialize(tx.signature)[0];

  console.log(pc.dim('signature ') + pc.green(signature) + pc.dim(' , asset : ') + pc.green(asset.publicKey));
  console.log(pc.cyan(getExplorerUrl(signature)));
  console.log(pc.cyan(getCoreExplorerUrl(asset.publicKey)));

  return { asset, signature };
}

// Standalone run: bun src/nft/nft_mint.ts
if (import.meta.main) {
  const umi = buildUmi();
  await mintNft(umi);
}
