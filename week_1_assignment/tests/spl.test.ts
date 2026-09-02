import { beforeAll, describe, expect, test } from 'bun:test';
import { address, type Address } from '@solana/kit';
import { litesvm } from '@solana/kit-plugin-litesvm';
import {
  fetchToken,
  fetchMint,
  findAssociatedTokenPda,
  TOKEN_PROGRAM_ADDRESS,
} from '@solana-program/token';
import {
  buildSplClient,
  initMint,
  mintToPayer,
  transferToRecipient,
} from '../src/spl.ts';

const MINT_AMOUNT = 2_000_000_000n;
const RECIPIENT = address('9aDCHGpiQdGber46fa2hXCC1wYp3bu4f28Tc5fV26fzo');

type SplClient = Awaited<ReturnType<typeof buildSplClient>>;

async function tokenBalance(client: SplClient, ata: Address) {
  const account = await fetchToken(client.rpc, ata, { commitment: 'confirmed' });
  return account.data.amount;
}

async function mintSupply(client: SplClient, mint: Address) {
  const account = await fetchMint(client.rpc, mint, { commitment: 'confirmed' });
  return account.data.supply;
}

// Single shared lifecycle: init -> mint -> transfer runs exactly once.
// Tests are ordered and share `client`/`mint`/`ata` so the flow is not duplicated.
describe('SPL token flow (single run)', () => {
  let client: SplClient;
  let mint: { address: Address };
  let ata: Address;

  beforeAll(async () => {
    client = await buildSplClient(litesvm());
  });

  test('initMint creates a mint with zero supply and derives ATA', async () => {
    const result = await initMint(client);
    mint = result.mint;
    ata = result.ata;

    expect(await mintSupply(client, mint.address)).toBe(0n);

    const [expectedAta] = await findAssociatedTokenPda({
      owner: client.payer.address,
      tokenProgram: TOKEN_PROGRAM_ADDRESS,
      mint: mint.address,
    });
    expect(ata).toBe(expectedAta);
  });

  test('mintToPayer mints into the payer ATA and raises supply', async () => {
    expect(mint).toBeDefined();
    await mintToPayer(client, mint);

    expect(await tokenBalance(client, ata)).toBe(MINT_AMOUNT);
    expect(await mintSupply(client, mint.address)).toBe(MINT_AMOUNT);
  });

  test('transferToRecipient moves full MINT_AMOUNT between ATAs', async () => {
    expect(mint).toBeDefined();
    await transferToRecipient(client, mint);

    const [sourceAta] = await findAssociatedTokenPda({
      owner: client.payer.address,
      tokenProgram: TOKEN_PROGRAM_ADDRESS,
      mint: mint.address,
    });
    const [destAta] = await findAssociatedTokenPda({
      owner: RECIPIENT,
      tokenProgram: TOKEN_PROGRAM_ADDRESS,
      mint: mint.address,
    });

    // src/spl.ts:68 transferToRecipient transfers MINT_AMOUNT (full balance)
    expect(await tokenBalance(client, sourceAta)).toBe(0n);
    expect(await tokenBalance(client, destAta)).toBe(MINT_AMOUNT);
    // supply unchanged by transfer
    expect(await mintSupply(client, mint.address)).toBe(MINT_AMOUNT);
  });
});
