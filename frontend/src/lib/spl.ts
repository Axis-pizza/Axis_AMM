import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  TransactionInstruction,
} from "@solana/web3.js";
import {
  ACCOUNT_SIZE,
  MINT_SIZE,
  TOKEN_PROGRAM_ID,
  createInitializeAccount3Instruction,
  createInitializeMint2Instruction,
  createMintToInstruction,
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountIdempotentInstruction,
  getMinimumBalanceForRentExemptAccount,
  getMinimumBalanceForRentExemptMint,
} from "@solana/spl-token";

/// Wallet-adapter-friendly SPL helpers.
///
/// `@solana/spl-token`'s `createMint` / `createAccount` shortcuts demand
/// a Keypair payer, which we don't have when the user signs through
/// Phantom. These helpers return the ix list + extra signers so the
/// React layer can build a single Transaction and hand it to
/// `wallet.sendTransaction(tx, conn, { signers: [...] })`.

export interface MintBundle {
  mint: PublicKey;
  mintKp: Keypair;
  ixs: TransactionInstruction[];
  signers: Keypair[];
}

/// Build a fresh SPL mint owned by `payer`, optionally minting an
/// initial supply into the payer's ATA. Caller composes the ixs into a
/// Transaction and signs with `[payer, mintKp]`.
export async function buildCreateMintWithSupplyIxs(
  conn: Connection,
  payer: PublicKey,
  decimals: number,
  initialSupply: bigint,
): Promise<MintBundle> {
  const mintKp = Keypair.generate();
  const mintRent = await getMinimumBalanceForRentExemptMint(conn);

  const ixs: TransactionInstruction[] = [
    SystemProgram.createAccount({
      fromPubkey: payer,
      newAccountPubkey: mintKp.publicKey,
      lamports: mintRent,
      space: MINT_SIZE,
      programId: TOKEN_PROGRAM_ID,
    }),
    createInitializeMint2Instruction(
      mintKp.publicKey,
      decimals,
      payer,
      payer,
      TOKEN_PROGRAM_ID,
    ),
  ];

  if (initialSupply > 0n) {
    const ata = getAssociatedTokenAddressSync(mintKp.publicKey, payer);
    ixs.push(
      createAssociatedTokenAccountIdempotentInstruction(
        payer,
        ata,
        payer,
        mintKp.publicKey,
      ),
      createMintToInstruction(mintKp.publicKey, ata, payer, initialSupply),
    );
  }

  return { mint: mintKp.publicKey, mintKp, ixs, signers: [mintKp] };
}

/// Build the ixs to allocate (but not initialize) a token account at a
/// random keypair address, so the program itself can call
/// InitializeAccount3 inside its handler. axis-vault and pfda-amm-3
/// both follow this pattern for vault accounts.
export async function buildBareTokenAccountIxs(
  conn: Connection,
  payer: PublicKey,
  count: number,
): Promise<{
  pubkeys: PublicKey[];
  signers: Keypair[];
  ixs: TransactionInstruction[];
}> {
  const rent = await getMinimumBalanceForRentExemptAccount(conn);
  const signers: Keypair[] = [];
  const ixs: TransactionInstruction[] = [];
  const pubkeys: PublicKey[] = [];
  for (let i = 0; i < count; i++) {
    const kp = Keypair.generate();
    signers.push(kp);
    pubkeys.push(kp.publicKey);
    ixs.push(
      SystemProgram.createAccount({
        fromPubkey: payer,
        newAccountPubkey: kp.publicKey,
        lamports: rent,
        space: ACCOUNT_SIZE,
        programId: TOKEN_PROGRAM_ID,
      }),
    );
  }
  return { pubkeys, signers, ixs };
}

/// Build a brand-new token account (not an ATA) for `mint`, owned by
/// `owner`, with caller-controlled keypair so it can be used in the
/// same tx as init logic. Useful for the ETF mint account where the
/// program initializes it itself.
export async function buildBareMintAccountIxs(
  conn: Connection,
  payer: PublicKey,
): Promise<{ pubkey: PublicKey; signer: Keypair; ixs: TransactionInstruction[] }> {
  const kp = Keypair.generate();
  const rent = await getMinimumBalanceForRentExemptMint(conn);
  return {
    pubkey: kp.publicKey,
    signer: kp,
    ixs: [
      SystemProgram.createAccount({
        fromPubkey: payer,
        newAccountPubkey: kp.publicKey,
        lamports: rent,
        space: MINT_SIZE,
        programId: TOKEN_PROGRAM_ID,
      }),
    ],
  };
}

/// Build "create + initialize SPL token account at known address owned by `owner`".
/// Used for the treasury ETF ATA + user ETF ATA where we want a deterministic
/// address (associated token account).
export function buildCreateAtaIfMissing(
  payer: PublicKey,
  owner: PublicKey,
  mint: PublicKey,
): { ata: PublicKey; ix: TransactionInstruction } {
  const ata = getAssociatedTokenAddressSync(mint, owner);
  return {
    ata,
    ix: createAssociatedTokenAccountIdempotentInstruction(payer, ata, owner, mint),
  };
}

/// Initialize a pre-allocated bare token account in-place. Used when
/// the program does NOT call InitializeAccount itself (rare; axis-vault
/// + pfmm both DO call it). Kept here as a generic helper.
export function ixInitTokenAccount(
  account: PublicKey,
  mint: PublicKey,
  owner: PublicKey,
): TransactionInstruction {
  return createInitializeAccount3Instruction(account, mint, owner, TOKEN_PROGRAM_ID);
}
