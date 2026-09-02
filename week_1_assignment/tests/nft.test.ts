import { beforeAll, describe, expect, test } from "bun:test";
import { publicKey } from "@metaplex-foundation/umi";
import { buildUmi, METADATA_URI } from "../src/nft/client";
import { mintNft } from "../src/nft/nft_mint";
import { updateNft } from "../src/nft/nft_update";
import { fetchAsset } from "../src/nft/nft_fetch";
import { burnNft } from "../src/nft/nft_burn";

// Reuses already-uploaded Arweave URIs (BS8y.../6ZeGF...).
// Image/metadata stays on Arweave forever (gateway.irys.xyz), only AssetV1 is on-chain.
// Run: bun test tests/nft.test.ts --timeout 120000
describe("NFT devnet flow", () => {
  let umi: ReturnType<typeof buildUmi>;
  let assetPubkey: string;

  beforeAll(() => {
    umi = buildUmi();
  });

  test(
    "mintNft creates asset with Pixel_pic and reused metadata URI",
    async () => {
      const { asset, signature } = await mintNft(umi);
      assetPubkey = String(asset.publicKey);

      expect(signature).toBeDefined();
      expect(assetPubkey).toMatch(/^[1-9A-HJ-NP-Za-km-z]{32,44}$/);

      const fetched: any = await fetchAsset(umi, assetPubkey);
      expect(fetched).not.toBeNull();
      expect(fetched.publicKey).toBeDefined();
      // mpl-core stores name/uri directly on AssetV1
      expect(fetched.name).toBe("Pixel_pic");
      expect(fetched.uri).toBe(METADATA_URI);
      expect(String(fetched.owner)).toBe(String(umi.identity.publicKey));
    },
    { timeout: 60_000 },
  );

  test(
    "updateNft changes name and uri as update authority",
    async () => {
      expect(assetPubkey).toBeDefined();
      const newName = "Pixel_pic Updated";
      const newUri = "https://gateway.irys.xyz/updated-metadata.json";

      const { signature, newName: retName, newUri: retUri } = await updateNft(umi, assetPubkey, {
        newName,
        newUri,
      });
      expect(signature).toBeDefined();
      expect(retName).toBe(newName);
      expect(retUri).toBe(newUri);

      // Devnet is eventually consistent — poll until update propagates
      let fetched: any = null;
      for (let i = 0; i < 12; i++) {
        fetched = await fetchAsset(umi, assetPubkey);
        if (fetched.name === newName && fetched.uri === newUri) break;
        await new Promise((r) => setTimeout(r, 800));
      }
      expect(fetched.name).toBe(newName);
      expect(fetched.uri).toBe(newUri);
    },
    { timeout: 60_000 },
  );

  test(
    "burn closes the account (Arweave URIs remain permanently — only on-chain burned)",
    async () => {
      expect(assetPubkey).toBeDefined();
      const sig = await burnNft(umi, assetPubkey);
      expect(sig).toBeDefined();

      // Devnet needs a few seconds to index the burn — poll until not found
      let threw = false;
      for (let i = 0; i < 12; i++) {
        try {
          await fetchAsset(umi, assetPubkey, { retries: 1 });
        } catch {
          threw = true;
          break;
        }
        await new Promise((r) => setTimeout(r, 800));
      }
      expect(threw).toBe(true);
    },
    { timeout: 60_000 },
  );
});
