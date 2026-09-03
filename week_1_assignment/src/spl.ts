// Init, Mint and Transfer SPL Token.
import {
  type Rpc,
  address,
  createClient,
  generateKeyPairSigner,
  lamports,
  type Address,
  type TransactionSigner,
} from "@solana/kit";
import { solanaLocalRpc } from "@solana/kit-plugin-rpc";
import { airdropSigner, signer } from "@solana/kit-plugin-signer";
import {
  tokenProgram,
  findAssociatedTokenPda,
  TOKEN_PROGRAM_ADDRESS,
  type TokenPlugin,
} from "@solana-program/token";
import pc from "./color";

const DECIMALS = 9;
const MINT_AMOUNT = 2_000_000_000n;
const RECIPIENT = address("9aDCHGpiQdGber46fa2hXCC1wYp3bu4f28Tc5fV26fzo");

export async function buildSplClient(
  transport: any = solanaLocalRpc(),
): Promise<FlowClient & { rpc: Rpc<any>; svm?: unknown }> {
  const wallet = await generateKeyPairSigner();
  const client: any = await (createClient() as any)
    .use(signer(wallet))
    .use(transport as any)
    .use(airdropSigner(lamports(1_000_000_000n)))
    .use(tokenProgram());
  return client;
}

export type SplClient = Awaited<ReturnType<typeof buildSplClient>>;

type FlowClient = { payer: TransactionSigner; identity: TransactionSigner; token: TokenPlugin };

export async function initMint(client: FlowClient) {
  const mint = await generateKeyPairSigner();
  const initResult = await client.token.instructions
    .createMint({
      newMint: mint,
      decimals: DECIMALS,
      mintAuthority: client.identity.address,
    })
    .sendTransaction();

  const [ata] = await findAssociatedTokenPda({
    owner: client.payer.address,
    tokenProgram: TOKEN_PROGRAM_ADDRESS,
    mint: mint.address,
  });

  console.log(pc.dim("wallet:") + pc.green(client.payer.address));
  console.log(pc.dim("mint:  ") + pc.green(mint.address));
  console.log(pc.dim("ata:   ") + pc.cyan(ata));
  console.log(pc.bold(pc.magenta("========== Init ==========")));
  console.log(
    pc.cyan(`https://explorer.solana.com/tx/${initResult.context.signature}?cluster=custom`),
  );

  return { mint, ata };
}

export async function mintToPayer(client: FlowClient, mint: { address: Address }) {
  const mintResult = await client.token.instructions
    .mintToATA({
      mint: mint.address,
      owner: client.payer.address,
      mintAuthority: client.payer,
      amount: MINT_AMOUNT,
      decimals: DECIMALS,
    })
    .sendTransaction();
  console.log(pc.bold(pc.magenta("========== Mint ==========")));
  console.log(
    pc.cyan(`https://explorer.solana.com/tx/${mintResult.context.signature}?cluster=custom`),
  );
}

export async function transferToRecipient(client: FlowClient, mint: { address: Address }) {
  const transferResult = await client.token.instructions
    .transferToATA({
      mint: mint.address,
      recipient: RECIPIENT,
      amount: MINT_AMOUNT,
      authority: client.payer,
      decimals: DECIMALS,
    })
    .sendTransaction();
  console.log(pc.bold(pc.magenta("========== Transfer ==========")));
  console.log(
    pc.cyan(`https://explorer.solana.com/tx/${transferResult.context.signature}?cluster=custom`),
  );
}

export async function runSplFlow(client: FlowClient) {
  const { mint, ata } = await initMint(client);
  await mintToPayer(client, mint);
  await transferToRecipient(client, mint);
  return { wallet: client.payer.address, mint: mint.address, ata };
}

// Script entry — runs only when executed directly, not on import by the test suite.
if (import.meta.main) {
  const client = await buildSplClient();
  await runSplFlow(client);
}
