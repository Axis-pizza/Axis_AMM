import { PublicKey } from "@solana/web3.js";

export type Cluster = "devnet" | "mainnet";

export interface ProgramRef {
  name: string;
  address: PublicKey;
  role: string;
  scope: "mainnet-v1" | "research" | "legacy";
}

export interface ClusterConfig {
  cluster: Cluster;
  label: string;
  rpcUrl: string;
  explorerCluster: "devnet" | "";
  jupiterEnabled: boolean;
  protocolTreasury?: PublicKey;
  programs: ProgramRef[];
}

const DEVNET_PROGRAMS: ProgramRef[] = [
  {
    name: "axis-vault",
    address: new PublicKey("Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX"),
    role: "ETF lifecycle (Create / Deposit / Withdraw / Sweep / SetFee / SetCap)",
    scope: "mainnet-v1",
  },
  {
    name: "pfda-amm-3",
    address: new PublicKey("3SBbfZgzAHyaijxbUbxBLt89aX6Z2d4ptL5PH6pzMazV"),
    role: "3-token PFDA batch auction with Switchboard oracle + Jito bid",
    scope: "mainnet-v1",
  },
];

const MAINNET_PROGRAMS: ProgramRef[] = [
  {
    name: "axis-vault",
    address: new PublicKey("Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX"),
    role: "ETF lifecycle (Create / Deposit / Withdraw / Sweep / SetFee / SetCap)",
    scope: "mainnet-v1",
  },
  {
    name: "pfda-amm-3",
    address: new PublicKey("3SBbfZgzAHyaijxbUbxBLt89aX6Z2d4ptL5PH6pzMazV"),
    role: "3-token PFDA batch auction with Switchboard oracle + Jito bid",
    scope: "mainnet-v1",
  },
];

export const MAINNET_PROTOCOL_TREASURY = new PublicKey(
  "BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6",
);

export function getClusterConfig(cluster: Cluster): ClusterConfig {
  if (cluster === "mainnet") {
    return {
      cluster,
      label: "mainnet-beta",
      rpcUrl: import.meta.env.VITE_MAINNET_RPC_URL ?? "https://api.mainnet-beta.solana.com",
      explorerCluster: "",
      jupiterEnabled: true,
      protocolTreasury: MAINNET_PROTOCOL_TREASURY,
      programs: MAINNET_PROGRAMS,
    };
  }

  return {
    cluster,
    label: "devnet",
    rpcUrl: import.meta.env.VITE_DEVNET_RPC_URL ?? "https://api.devnet.solana.com",
    explorerCluster: "devnet",
    jupiterEnabled: false,
    protocolTreasury: MAINNET_PROTOCOL_TREASURY,
    programs: DEVNET_PROGRAMS,
  };
}
