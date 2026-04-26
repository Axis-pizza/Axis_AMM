/// Truncate a base58 pubkey for compact display.
/// 11111111111111111111111111111111 → 11111…1111
export function truncatePubkey(pk: string, head = 5, tail = 4): string {
  if (pk.length <= head + tail + 1) return pk;
  return `${pk.slice(0, head)}…${pk.slice(-tail)}`;
}

/// Lamports → SOL with 4 decimals, no scientific notation surprises.
export function lamportsToSol(lamports: number | bigint): string {
  const n = typeof lamports === "bigint" ? Number(lamports) : lamports;
  return (n / 1_000_000_000).toFixed(4);
}

/// Format a u64 byte count as KB / MB so the program-data size column
/// stays readable.
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}
