import { Connection, PublicKey } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";

/// Devnet wallet token holdings. We deliberately do NOT call Jupiter
/// here — the Jupiter token list is mainnet-only, and resolving devnet
/// SPL mints via off-chain APIs returns junk. So this returns just the
/// mint + balance + decimals straight from
/// `getParsedTokenAccountsByOwner`, and the UI labels them by truncated
/// mint with a heuristic for the well-known wrapped-SOL mint.

export interface WalletToken {
  mint: PublicKey;
  ata: PublicKey;
  amount: bigint;
  decimals: number;
  uiAmount: number;
  /// Best-effort short label, used purely as UI hint.
  label: string;
}

const WSOL_MINT = "So11111111111111111111111111111111111111112";

function shortLabel(mint: string): string {
  if (mint === WSOL_MINT) return "wSOL";
  return `mint:${mint.slice(0, 4)}…${mint.slice(-4)}`;
}

export async function fetchWalletTokens(
  conn: Connection,
  owner: PublicKey,
): Promise<WalletToken[]> {
  const res = await conn.getParsedTokenAccountsByOwner(owner, {
    programId: TOKEN_PROGRAM_ID,
  });

  return res.value
    .map((acct) => {
      const info = (acct.account.data as { parsed: { info: ParsedTokenInfo } })
        .parsed.info;
      const mint = new PublicKey(info.mint);
      const decimals = info.tokenAmount.decimals;
      const amount = BigInt(info.tokenAmount.amount);
      const uiAmount = info.tokenAmount.uiAmount ?? 0;
      return {
        mint,
        ata: acct.pubkey,
        amount,
        decimals,
        uiAmount,
        label: shortLabel(info.mint),
      };
    })
    .filter((t) => t.amount > 0n)
    .sort((a, b) => b.uiAmount - a.uiAmount);
}

interface ParsedTokenInfo {
  mint: string;
  owner: string;
  tokenAmount: {
    amount: string;
    decimals: number;
    uiAmount: number | null;
    uiAmountString: string;
  };
}
