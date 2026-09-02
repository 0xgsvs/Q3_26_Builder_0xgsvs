import { createUmi } from "@metaplex-foundation/umi-bundle-defaults";
import { mplCore } from "@metaplex-foundation/mpl-core";
import { createSignerFromKeypair, signerIdentity, type Umi } from "@metaplex-foundation/umi";
import { irysUploader } from "@metaplex-foundation/umi-uploader-irys";
import wallet from "../../devnet-wallet.json";

// Reuse already-uploaded Arweave URIs — no need to re-upload every run.
// Arweave is permanent (gateway.irys.xyz is chain-agnostic, not devnet-specific).
export const IMAGE_URI = "https://gateway.irys.xyz/BS8yVgsk5TnZ7vXnnsAWR9Gqr8v1QAAkzjM8UZn89nx";
export const METADATA_URI = "https://gateway.irys.xyz/6ZeGF2XYM7Bv8VE7kkt7xhjc2TKawrproB7xWATMT2aH";
export const NFT_NAME = "Pixel_pic";

function getIrysAddress(rpcUrl: string): string {
  if (rpcUrl.includes("mainnet")) return "https://node1.irys.xyz";
  return "https://devnet.irys.xyz";
}

export function buildUmi(
  rpcUrl: string = process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com",
): Umi {
  const umi = createUmi(rpcUrl)
    .use(mplCore())
    .use(irysUploader({ address: getIrysAddress(rpcUrl) }));

  const keypair = umi.eddsa.createKeypairFromSecretKey(new Uint8Array(wallet as number[]));
  const signer = createSignerFromKeypair(umi, keypair);
  umi.use(signerIdentity(signer));

  return umi;
}

export function getExplorerUrl(signature: string, rpcUrl?: string): string {
  const url = rpcUrl ?? process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";
  const cluster = url.includes("mainnet") ? "mainnet-beta" : url.includes("devnet") ? "devnet" : "custom&customUrl=http://localhost:8899";
  return `https://explorer.solana.com/tx/${signature}?cluster=${cluster}`;
}

export function getCoreExplorerUrl(asset: string, rpcUrl?: string): string {
  const url = rpcUrl ?? process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";
  const env = url.includes("mainnet") ? "mainnet" : "devnet";
  return `https://core.metaplex.com/explorer/${asset}?env=${env}`;
}
