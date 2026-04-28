import { useMemo, type ReactNode } from "react";
import { ConnectionProvider, WalletProvider } from "@solana/wallet-adapter-react";
import { WalletModalProvider } from "@solana/wallet-adapter-react-ui";
import { PhantomWalletAdapter, SolflareWalletAdapter } from "@solana/wallet-adapter-wallets";

/// Wraps the demo in the @solana/wallet-adapter providers. Deliberately
/// only registers Phantom + Solflare — adding Backpack/Glow/etc bloats
/// the modal without helping a 2-program devnet demo.
export function AppWalletProvider({
  children,
  endpoint,
}: {
  children: ReactNode;
  endpoint: string;
}) {
  const wallets = useMemo(
    () => [new PhantomWalletAdapter(), new SolflareWalletAdapter()],
    [],
  );

  return (
    <ConnectionProvider key={endpoint} endpoint={endpoint}>
      <WalletProvider wallets={wallets} autoConnect>
        <WalletModalProvider>{children}</WalletModalProvider>
      </WalletProvider>
    </ConnectionProvider>
  );
}
