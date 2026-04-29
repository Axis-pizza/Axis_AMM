import { describe, expect, test } from "bun:test";
import { PublicKey } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";

import { ixDeposit, ixWithdraw } from "../../frontend/src/lib/ix";

const PROGRAM_ID = new PublicKey("Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX");

const PAYER = new PublicKey("BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6");
const ETF_STATE = new PublicKey("11111111111111111111111111111112");
const ETF_MINT = new PublicKey("11111111111111111111111111111113");
const USER_ETF_ATA = new PublicKey("11111111111111111111111111111114");
const TREASURY_ETF_ATA = new PublicKey("11111111111111111111111111111115");
const VAULT_0 = new PublicKey("11111111111111111111111111111116");
const VAULT_1 = new PublicKey("11111111111111111111111111111117");
const USER_BASKET_0 = new PublicKey("11111111111111111111111111111118");
const USER_BASKET_1 = new PublicKey("11111111111111111111111111111119");

describe("ixDeposit (axis-vault disc=1)", () => {
  test("encodes account order: signer, etf_state, etf_mint, user_etf_ata, token_program, treasury_etf_ata, user_basket[..], vault[..]", () => {
    const ix = ixDeposit({
      programId: PROGRAM_ID,
      payer: PAYER,
      etfState: ETF_STATE,
      etfMint: ETF_MINT,
      userEtfAta: USER_ETF_ATA,
      treasuryEtfAta: TREASURY_ETF_ATA,
      userBasketAccounts: [USER_BASKET_0, USER_BASKET_1],
      vaults: [VAULT_0, VAULT_1],
      amount: 1_000_000n,
      minMintOut: 0n,
      name: "AX",
    });
    expect(ix.programId.equals(PROGRAM_ID)).toBe(true);
    expect(ix.keys.map((k) => k.pubkey.toBase58())).toEqual([
      PAYER.toBase58(),
      ETF_STATE.toBase58(),
      ETF_MINT.toBase58(),
      USER_ETF_ATA.toBase58(),
      TOKEN_PROGRAM_ID.toBase58(),
      TREASURY_ETF_ATA.toBase58(),
      USER_BASKET_0.toBase58(),
      USER_BASKET_1.toBase58(),
      VAULT_0.toBase58(),
      VAULT_1.toBase58(),
    ]);
    // disc=1, amount=1_000_000 LE, minMintOut=0 LE, name_len=2, "AX"
    expect(ix.data[0]).toBe(1);
    expect(ix.data.readBigUInt64LE(1)).toBe(1_000_000n);
    expect(ix.data.readBigUInt64LE(9)).toBe(0n);
    expect(ix.data[17]).toBe(2);
    expect(ix.data.slice(18).toString("utf8")).toBe("AX");
  });
});

describe("ixWithdraw (axis-vault disc=2)", () => {
  test("encodes account order: signer, etf_state, etf_mint, user_etf_ata, token_program, treasury_etf_ata, vault[..], user_basket[..]", () => {
    const ix = ixWithdraw({
      programId: PROGRAM_ID,
      payer: PAYER,
      etfState: ETF_STATE,
      etfMint: ETF_MINT,
      userEtfAta: USER_ETF_ATA,
      treasuryEtfAta: TREASURY_ETF_ATA,
      vaults: [VAULT_0, VAULT_1],
      userBasketAccounts: [USER_BASKET_0, USER_BASKET_1],
      burnAmount: 500_000n,
      minTokensOut: 9_000n,
      name: "AX",
    });
    expect(ix.programId.equals(PROGRAM_ID)).toBe(true);
    // Vault accounts must come BEFORE user basket ATAs — that's the
    // reverse of Deposit's layout. The on-chain handler at
    // contracts/axis-vault/src/instructions/withdraw.rs reads vaults at
    // [6+i] and user dests at [6+N+i]; an inverted layout would silently
    // transfer to the wrong accounts.
    expect(ix.keys.map((k) => k.pubkey.toBase58())).toEqual([
      PAYER.toBase58(),
      ETF_STATE.toBase58(),
      ETF_MINT.toBase58(),
      USER_ETF_ATA.toBase58(),
      TOKEN_PROGRAM_ID.toBase58(),
      TREASURY_ETF_ATA.toBase58(),
      VAULT_0.toBase58(),
      VAULT_1.toBase58(),
      USER_BASKET_0.toBase58(),
      USER_BASKET_1.toBase58(),
    ]);
    expect(ix.data[0]).toBe(2);
    expect(ix.data.readBigUInt64LE(1)).toBe(500_000n);
    expect(ix.data.readBigUInt64LE(9)).toBe(9_000n);
    expect(ix.data[17]).toBe(2);
    expect(ix.data.slice(18).toString("utf8")).toBe("AX");
    // Sanity: signers + writability mirror on-chain expectations.
    expect(ix.keys[0].isSigner).toBe(true);
    expect(ix.keys[0].isWritable).toBe(true);
    expect(ix.keys[1].isWritable).toBe(true); // etf_state
    expect(ix.keys[2].isWritable).toBe(true); // etf_mint
    expect(ix.keys[3].isWritable).toBe(true); // user_etf_ata
    expect(ix.keys[4].isWritable).toBe(false); // token_program
    expect(ix.keys[5].isWritable).toBe(true); // treasury_etf_ata
    for (let i = 6; i < ix.keys.length; i++) {
      expect(ix.keys[i].isSigner).toBe(false);
      expect(ix.keys[i].isWritable).toBe(true);
    }
  });

  test("rejects mismatched vaults / user-basket lengths", () => {
    expect(() =>
      ixWithdraw({
        programId: PROGRAM_ID,
        payer: PAYER,
        etfState: ETF_STATE,
        etfMint: ETF_MINT,
        userEtfAta: USER_ETF_ATA,
        treasuryEtfAta: TREASURY_ETF_ATA,
        vaults: [VAULT_0, VAULT_1],
        userBasketAccounts: [USER_BASKET_0],
        burnAmount: 1n,
        minTokensOut: 0n,
        name: "AX",
      }),
    ).toThrow(/length mismatch/);
  });
});
