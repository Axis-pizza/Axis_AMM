import {
  AddressLookupTableAccount,
  Connection,
  PublicKey,
  TransactionInstruction,
  type AccountMeta,
} from "@solana/web3.js";

export const SOL_MINT = new PublicKey("So11111111111111111111111111111111111111112");
export const DEFAULT_JUPITER_HOST = "https://quote-api.jup.ag";

export type SwapMode = "ExactIn" | "ExactOut";

export interface JupiterQuoteParams {
  inputMint: PublicKey;
  outputMint: PublicKey;
  amount: bigint;
  slippageBps: number;
  swapMode?: SwapMode;
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
  data: string;
}

export interface JupiterSwapInstructionsResponse {
  computeBudgetInstructions: SerializedIx[];
  setupInstructions: SerializedIx[];
  swapInstruction: SerializedIx;
  cleanupInstruction?: SerializedIx | null;
  addressLookupTableAddresses: string[];
}

export async function getQuote(params: JupiterQuoteParams): Promise<JupiterQuoteResponse> {
  const host = params.jupiterHost ?? DEFAULT_JUPITER_HOST;
  const qs = new URLSearchParams({
    inputMint: params.inputMint.toBase58(),
    outputMint: params.outputMint.toBase58(),
    amount: params.amount.toString(),
    slippageBps: params.slippageBps.toString(),
    swapMode: params.swapMode ?? "ExactIn",
    maxAccounts: (params.maxAccounts ?? 24).toString(),
    onlyDirectRoutes: "false",
  });
  const res = await fetch(`${host}/v6/quote?${qs.toString()}`);
  if (!res.ok) {
    throw new Error(`Jupiter quote failed: ${res.status} ${await res.text()}`);
  }
  return (await res.json()) as JupiterQuoteResponse;
}

export async function getSwapInstructions(params: {
  quote: JupiterQuoteResponse;
  userPublicKey: PublicKey;
  wrapAndUnwrapSol?: boolean;
  destinationTokenAccount?: PublicKey;
  jupiterHost?: string;
}): Promise<JupiterSwapInstructionsResponse> {
  const host = params.jupiterHost ?? DEFAULT_JUPITER_HOST;
  const res = await fetch(`${host}/v6/swap-instructions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      quoteResponse: params.quote,
      userPublicKey: params.userPublicKey.toBase58(),
      wrapAndUnwrapSol: params.wrapAndUnwrapSol ?? true,
      destinationTokenAccount: params.destinationTokenAccount?.toBase58(),
      useSharedAccounts: true,
    }),
  });
  if (!res.ok) {
    throw new Error(`Jupiter swap-instructions failed: ${res.status} ${await res.text()}`);
  }
  return (await res.json()) as JupiterSwapInstructionsResponse;
}

export function deserializeIx(raw: SerializedIx): TransactionInstruction {
  const keys: AccountMeta[] = raw.accounts.map((a) => ({
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

export async function fetchAltAccounts(
  conn: Connection,
  addresses: string[],
): Promise<AddressLookupTableAccount[]> {
  const uniq = Array.from(new Set(addresses));
  if (uniq.length === 0) return [];
  const infos = await conn.getMultipleAccountsInfo(uniq.map((a) => new PublicKey(a)));
  const out: AddressLookupTableAccount[] = [];
  for (let i = 0; i < uniq.length; i++) {
    const info = infos[i];
    if (!info) continue;
    out.push(
      new AddressLookupTableAccount({
        key: new PublicKey(uniq[i]),
        state: AddressLookupTableAccount.deserialize(info.data),
      }),
    );
  }
  return out;
}
