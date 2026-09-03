import { publicKey, some, type PublicKey, type Umi } from "@metaplex-foundation/umi";
import { updateV1 } from "@metaplex-foundation/mpl-core";
import { base58 } from "@metaplex-foundation/umi/serializers";
import pc from "../color";
import { buildUmi, getExplorerUrl } from "./client";

export async function updateNft(
  umi: Umi,
  asset: PublicKey | string,
  opts?: { newName?: string; newUri?: string },
): Promise<{ signature: string; newName: string; newUri: string }> {
  const newName = opts?.newName ?? "Pixel_pic Updated";
  const newUri = opts?.newUri ?? "https://gateway.irys.xyz/updated-metadata.json";
  const assetPk = publicKey(asset as string);

  const result = await (updateV1 as any)(umi, {
    asset: assetPk,
    newName: some(newName),
    newUri: some(newUri),
  }).sendAndConfirm(umi);

  const signature = base58.deserialize(result.signature)[0];
  console.log(
    pc.dim("updated asset:") +
      pc.green(String(assetPk)) +
      pc.dim(" newName:") +
      pc.yellow(newName) +
      pc.dim(" newUri:") +
      pc.cyan(newUri),
  );
  console.log(pc.cyan(getExplorerUrl(signature)));

  return { signature, newName, newUri };
}

// Standalone run: bun src/nft/nft_update.ts <assetPubkey>
if (import.meta.main) {
  const umi = buildUmi();
  const asset = process.argv[2];
  if (!asset) throw new Error("Usage: bun src/nft/nft_update.ts <assetPubkey>");
  await updateNft(umi, asset);
}
