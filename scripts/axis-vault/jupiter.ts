/**
 * Jupiter V6 Swap API client for Axis Vault SOL-in / SOL-out flows (#36).
 *
 * We deliberately do NOT CPI into Jupiter from the axis-vault program.
 * Instead, the client assembles a single versioned transaction that
 * bundles:
 *
 *   [ComputeBudget][Jupiter swap × N][axis-vault Deposit]        (SOL in)
 *   [ComputeBudget][axis-vault Withdraw][Jupiter swap × N][Close] (SOL out)
 *
 * Same atomicity as on-chain CPI (revert is all-or-nothing within a
 * single tx) with none of the downsides: no blow-up of the program
 * account list, no 1 M+ CU budget for N × 200 k-CU Jupiter routes, no
 * bespoke ALT plumbing inside the program. This matches how production
 * basket / ETF protocols on Solana (Kamino, Drift, Marginfi vaults,
 * etc.) integrate Jupiter.
 *
 * All types/endpoints follow the public Jupiter V6 API:
 *   https://station.jup.ag/docs/apis/swap-api
 */
import {
  AccountMeta,
  AddressLookupTableAccount,
  Connection,
  PublicKey,
  TransactionInstruction,
} from "@solana/web3.js";

export const JUPITER_V6_PROGRAM_ID = new PublicKey(
  "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4",
);
export const SOL_MINT = new PublicKey(
  "So11111111111111111111111111111111111111112",
);
export const DEFAULT_JUPITER_HOST = "https://quote-api.jup.ag";

export type SwapMode = "ExactIn" | "ExactOut";

export interface JupiterQuoteParams {
  inputMint: PublicKey;
  outputMint: PublicKey;
  /** Raw base-unit amount (lamports for SOL, atoms for SPL tokens). */
  amount: bigint;
  /** e.g. 50 = 0.5 %. Applied by Jupiter as `otherAmountThreshold`. */
  slippageBps: number;
  swapMode?: SwapMode;
  /** Hard cap on intermediate hops — fewer hops = smaller tx. */
  maxAccounts?: number;
  jupiterHost?: string;
}

export interface JupiterQuoteResponse {
  inputMint: string;
  outputMint: string;
  inAmount: string;
  outAmount: string;
  otherAmountThreshold: string;
  swapMode: SwapMode;
  slippageBps: number;
  priceImpactPct: string;
  routePlan: unknown[];
  contextSlot?: number;
}

interface SerializedIxAccount {
  pubkey: string;
  isSigner: boolean;
  isWritable: boolean;
}
interface SerializedIx {
  programId: string;
  accounts: SerializedIxAccount[];
  data: string; // base64
}

export interface JupiterSwapInstructionsResponse {
  tokenLedgerInstruction?: SerializedIx | null;
  computeBudgetInstructions: SerializedIx[];
  setupInstructions: SerializedIx[];
  swapInstruction: SerializedIx;
  cleanupInstruction?: SerializedIx | null;
  addressLookupTableAddresses: string[];
}

/** Ask Jupiter for a quote. Returns the parsed JSON response. */
export async function getQuote(params: JupiterQuoteParams): Promise<JupiterQuoteResponse> {
  const host = params.jupiterHost ?? DEFAULT_JUPITER_HOST;
  const qs = new URLSearchParams({
    inputMint: params.inputMint.toBase58(),
    outputMint: params.outputMint.toBase58(),
    amount: params.amount.toString(),
    slippageBps: params.slippageBps.toString(),
    swapMode: params.swapMode ?? "ExactIn",
    // Cap the account list so N legs fit under the versioned tx limit.
    // 24 is Jupiter's recommended default for bundled flows.
    maxAccounts: (params.maxAccounts ?? 24).toString(),
    onlyDirectRoutes: "false",
  });
  const url = `${host}/v6/quote?${qs.toString()}`;
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`Jupiter quote failed: ${res.status} ${await res.text()}`);
  }
  return (await res.json()) as JupiterQuoteResponse;
}

export interface JupiterSwapInstructionsParams {
  quote: JupiterQuoteResponse;
  userPublicKey: PublicKey;
  /**
   * When true, Jupiter will emit setup/cleanup IXes to wrap/unwrap SOL
   * around the swap. Set to false when you're already managing a shared
   * wSOL ATA across multiple legs (we do that here — see
   * `buildDepositSolPlan`).
   */
  wrapAndUnwrapSol?: boolean;
  /** Token account that will hold the output, if not the default ATA. */
  destinationTokenAccount?: PublicKey;
  jupiterHost?: string;
}

/** Ask Jupiter for ready-to-use IXes for a quoted swap. */
export async function getSwapInstructions(
  params: JupiterSwapInstructionsParams,
): Promise<JupiterSwapInstructionsResponse> {
  const host = params.jupiterHost ?? DEFAULT_JUPITER_HOST;
  const body = {
    quoteResponse: params.quote,
    userPublicKey: params.userPublicKey.toBase58(),
    wrapAndUnwrapSol: params.wrapAndUnwrapSol ?? true,
    destinationTokenAccount: params.destinationTokenAccount?.toBase58(),
    // Use a shared wSOL account so N legs don't each re-wrap.
    useSharedAccounts: true,
  };
  const res = await fetch(`${host}/v6/swap-instructions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    throw new Error(`Jupiter swap-instructions failed: ${res.status} ${await res.text()}`);
  }
  return (await res.json()) as JupiterSwapInstructionsResponse;
}

/** Convert a Jupiter-serialized IX into a web3.js TransactionInstruction. */
export function deserializeIx(raw: SerializedIx): TransactionInstruction {
  const keys: AccountMeta[] = raw.accounts.map(a => ({
    pubkey: new PublicKey(a.pubkey),
    isSigner: a.isSigner,
    isWritable: a.isWritable,
  }));
  return new TransactionInstruction({
    programId: new PublicKey(raw.programId),
    keys,
    data: Buffer.from(raw.data, "base64"),
  });
}

/**
 * Fetch and deserialize every AddressLookupTable referenced across one
 * or more Jupiter responses. Caller passes the union to
 * `TransactionMessage.compileToV0Message` so the resulting tx can pack
 * Jupiter's large account lists into the 1232-byte envelope.
 */
export async function fetchAltAccounts(
  conn: Connection,
  addresses: string[],
): Promise<AddressLookupTableAccount[]> {
  const uniq = Array.from(new Set(addresses));
  if (uniq.length === 0) return [];
  const infos = await conn.getMultipleAccountsInfo(uniq.map(a => new PublicKey(a)));
  const out: AddressLookupTableAccount[] = [];
  for (let i = 0; i < uniq.length; i++) {
    const ai = infos[i];
    if (!ai) continue;
    out.push(
      new AddressLookupTableAccount({
        key: new PublicKey(uniq[i]),
        state: AddressLookupTableAccount.deserialize(ai.data),
      }),
    );
  }
  return out;
}
