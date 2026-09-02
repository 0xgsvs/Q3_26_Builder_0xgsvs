import { createGenericFile, type Umi } from "@metaplex-foundation/umi";
import { readFile } from "fs/promises";
import pc from "picocolors";
import { buildUmi } from "./client";

// Uploads image to Irys (Arweave) and returns gateway URI.
// Reuse is cheap: Arweave is permanent, gateway.irys.xyz is chain-agnostic.
export async function uploadImage(
  umi: Umi,
  imagePath: string = "/home/greed/projects/solana/Q3_26_Builder_0xgsvs/week_1_assignment/pixelnft.png",
): Promise<string> {
  const image = await readFile(imagePath);
  const file = createGenericFile(image, "pixelnft.png", { contentType: "image/png" });
  const [myUri] = await umi.uploader.upload([file]);
  if (!myUri) throw new Error("Irys upload failed: no URI");
  console.log(pc.dim('Your image URI: ') + pc.cyan(myUri));
  return myUri;
}

// Standalone run: bun src/nft/nft_image.ts
if (import.meta.main) {
  const umi = buildUmi();
  await uploadImage(umi);
}

// https://gateway.irys.xyz/BS8yVgsk5TnZ7vXnnsAWR9Gqr8v1QAAkzjM8UZn89nx
