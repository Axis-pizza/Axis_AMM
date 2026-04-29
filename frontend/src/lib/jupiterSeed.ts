import { PublicKey } from "@solana/web3.js";
import {
  SOL_MINT,
  getQuote,
  type JupiterQuoteParams,
  type JupiterQuoteResponse,
} from "./jupiter";

export type JupiterQuoteMode = "live" | "mock";

export interface JupiterQuoteClient {
  mode: JupiterQuoteMode;
  getQuote(params: JupiterQuoteParams): Promise<JupiterQuoteResponse>;
}

export interface JupiterSeedPreviewArgs {
  basketMints: Array<PublicKey | string>;
  weights: number[];
  solIn: bigint;
  slippageBps?: number;
  /// Cap on accounts in each leg's swap ix. Lower values force Jupiter
  /// to pick simpler routes (fewer hops/AMMs), shrinking the eventual
  /// transaction. 16 is the practical default for multi-leg flows that
  /// must coexist with our own ATA/wrap/Deposit ixs in a single tx
  /// under the 1232-byte wire cap.
  maxAccounts?: number;
  quoteClient?: JupiterQuoteClient;
}

export interface JupiterSeedLegPreview {
  mint: PublicKey;
  weightBps: number;
  solLamports: bigint;
  quote: JupiterQuoteResponse;
  expectedOut: bigint;
  minOut: bigint;
  depositCandidate: bigint;
  routeLabel: string;
}

export interface JupiterSeedPreview {
  mode: JupiterQuoteMode;
  solIn: bigint;
  slippageBps: number;
  depositAmount: bigint;
  bottleneckIndex: number;
  legs: JupiterSeedLegPreview[];
}

export const liveJupiterQuoteClient: JupiterQuoteClient = {
  mode: "live",
  getQuote,
};

export function createMockJupiterQuoteClient({
  outputBpsByMint = {},
  defaultOutputBps = 85_000,
}: {
  outputBpsByMint?: Record<string, number>;
  defaultOutputBps?: number;
} = {}): JupiterQuoteClient {
  return {
    mode: "mock",
    async getQuote(params) {
      const outputMint = params.outputMint.toBase58();
      const outputBps = outputBpsByMint[outputMint] ?? defaultOutputBps;
      const outAmount = (params.amount * BigInt(outputBps)) / 10_000n;
      const minOut =
        (outAmount * BigInt(10_000 - params.slippageBps)) / 10_000n;

      return {
        inputMint: params.inputMint.toBase58(),
        outputMint,
        inAmount: params.amount.toString(),
        outAmount: outAmount.toString(),
        otherAmountThreshold: minOut.toString(),
        swapMode: params.swapMode ?? "ExactIn",
        slippageBps: params.slippageBps,
        priceImpactPct: "0",
        routePlan: [{ swapInfo: { label: "MockJup" }, percent: 100 }],
        contextSlot: 0,
      };
    },
  };
}

export async function buildJupiterSeedPreview({
  basketMints,
  weights,
  solIn,
  slippageBps = 50,
  maxAccounts = 16,
  quoteClient = liveJupiterQuoteClient,
}: JupiterSeedPreviewArgs): Promise<JupiterSeedPreview> {
  const mints = basketMints.map((mint) =>
    mint instanceof PublicKey ? mint : new PublicKey(mint),
  );
  validateSeedInputs(mints, weights, solIn);

  const legLamports = splitWeightedLamports(solIn, weights);
  const quoteResults = await Promise.all(
    mints.map((mint, i) =>
      quoteClient.getQuote({
        inputMint: SOL_MINT,
        outputMint: mint,
        amount: legLamports[i],
        slippageBps,
        swapMode: "ExactIn",
        maxAccounts,
      }),
    ),
  );

  const legs = quoteResults.map((quote, i) => {
    const minOut = BigInt(quote.otherAmountThreshold);
    return {
      mint: mints[i],
      weightBps: weights[i],
      solLamports: legLamports[i],
      quote,
      expectedOut: BigInt(quote.outAmount),
      minOut,
      depositCandidate: (minOut * 10_000n) / BigInt(weights[i]),
      routeLabel: extractRouteLabel(quote),
    };
  });

  let bottleneckIndex = 0;
  for (let i = 1; i < legs.length; i++) {
    if (legs[i].depositCandidate < legs[bottleneckIndex].depositCandidate) {
      bottleneckIndex = i;
    }
  }

  return {
    mode: quoteClient.mode,
    solIn,
    slippageBps,
    depositAmount: legs[bottleneckIndex].depositCandidate,
    bottleneckIndex,
    legs,
  };
}

function validateSeedInputs(
  basketMints: PublicKey[],
  weights: number[],
  solIn: bigint,
) {
  if (basketMints.length !== weights.length) {
    throw new Error("basketMints / weights length mismatch");
  }
  if (basketMints.length < 2 || basketMints.length > 5) {
    throw new Error("basket size must be 2..5");
  }
  const weightSum = weights.reduce((a, b) => a + b, 0);
  if (weightSum !== 10_000) {
    throw new Error(`weights must sum to 10_000, got ${weightSum}`);
  }
  if (weights.some((w) => w <= 0)) {
    throw new Error("weights must be positive");
  }
  if (solIn <= 0n) {
    throw new Error("SOL input must be greater than zero");
  }
}

function splitWeightedLamports(solIn: bigint, weights: number[]): bigint[] {
  const legs = weights.map((w) => (solIn * BigInt(w)) / 10_000n);
  const assigned = legs.reduce((sum, lamports) => sum + lamports, 0n);
  legs[legs.length - 1] += solIn - assigned;
  return legs;
}

function extractRouteLabel(quote: JupiterQuoteResponse): string {
  const first = quote.routePlan[0];
  if (
    typeof first === "object" &&
    first !== null &&
    "swapInfo" in first &&
    typeof first.swapInfo === "object" &&
    first.swapInfo !== null &&
    "label" in first.swapInfo &&
    typeof first.swapInfo.label === "string"
  ) {
    return first.swapInfo.label;
  }
  return "Jupiter";
}
