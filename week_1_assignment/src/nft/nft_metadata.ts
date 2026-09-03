import type { Umi } from "@metaplex-foundation/umi";
import pc from "../color";
import { buildUmi, IMAGE_URI } from "./client";

export async function uploadMetadata(
  umi: Umi,
  opts?: { image?: string; name?: string; description?: string },
): Promise<string> {
  const image = opts?.image ?? IMAGE_URI;
  const metadata = {
    name: opts?.name ?? "Pixel_pic",
    description: opts?.description ?? "A Pixel image",
    image,
    category: "image" as const,
  };

  const myUri = await umi.uploader.uploadJson(metadata);
  console.log(pc.dim("metadata uri: ") + pc.cyan(myUri));
  return myUri;
}

// Standalone run: bun src/nft/nft_metadata.ts
if (import.meta.main) {
  const umi = buildUmi();
  await uploadMetadata(umi);
}
// metadata uri: https://gateway.irys.xyz/6ZeGF2XYM7Bv8VE7kkt7xhjc2TKawrproB7xWATMT2aH
