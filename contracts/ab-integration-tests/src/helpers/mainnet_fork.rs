use litesvm::LiteSVM;
use serde::Deserialize;
use solana_address::Address;
use solana_rpc_client::rpc_client::RpcClient;

use solana_account::Account;

/// Solana runtime permits up to 10KB realloc per CPI call.
const MAX_PERMITTED_DATA_INCREASE: usize = 10_240;

/// Clone an account from mainnet RPC into LiteSVM.
pub fn clone_from_rpc(svm: &mut LiteSVM, rpc: &RpcClient, addr: &Address) -> bool {
    match rpc.get_account(addr) {
        Ok(account) => {
            svm.set_account(*addr, account).unwrap();
            true
        }
        Err(e) => {
            eprintln!("  warn: clone failed {}: {}", addr, e);
            false
        }
    }
}

/// Clone an account with extra data padding to allow realloc in CPI.
///
/// Jupiter's SharedAccountsRoute reallocates intermediate token accounts
/// during swaps. In LiteSVM, `set_account` fixes the data size — the runtime
/// won't allow growing beyond it. We pad writable accounts with zero bytes
/// to give the CPI room to realloc.
pub fn clone_with_realloc_padding(
    svm: &mut LiteSVM,
    rpc: &RpcClient,
    addr: &Address,
    extra_bytes: usize,
) -> bool {
    match rpc.get_account(addr) {
        Ok(mut account) => {
            account.data.resize(account.data.len() + extra_bytes, 0);
            svm.set_account(*addr, account).unwrap();
            true
        }
        Err(e) => {
            eprintln!("  warn: clone failed {}: {}", addr, e);
            false
        }
    }
}

/// Clone multiple accounts in batch.
pub fn clone_accounts_batch(svm: &mut LiteSVM, rpc: &RpcClient, addrs: &[Address]) -> usize {
    let mut cloned = 0;
    for addr in addrs {
        if clone_from_rpc(svm, rpc, addr) {
            cloned += 1;
        }
    }
    cloned
}

// ─── Jupiter API types ──────────────────────────────────────────────────

#[derive(Debug)]
pub struct JupiterRoute {
    pub swap_data: Vec<u8>,
    pub accounts: Vec<JupiterAccount>,
    pub address_lookup_tables: Vec<Address>,
    pub in_amount: u64,
    pub out_amount: u64,
}

#[derive(Debug)]
pub struct JupiterAccount {
    pub pubkey: Address,
    pub is_signer: bool,
    pub is_writable: bool,
}

#[derive(Deserialize)]
struct QuoteResponse {
    #[serde(rename = "inAmount")]
    in_amount: String,
    #[serde(rename = "outAmount")]
    out_amount: String,
}

#[derive(Deserialize)]
struct SwapInstructionsResponse {
    #[serde(rename = "swapInstruction")]
    swap_instruction: SwapInstruction,
    #[serde(rename = "addressLookupTableAddresses", default)]
    address_lookup_table_addresses: Vec<String>,
}

#[derive(Deserialize)]
struct SwapInstruction {
    #[serde(rename = "programId")]
    _program_id: String,
    accounts: Vec<SwapAccount>,
    data: String,
}

#[derive(Deserialize)]
struct SwapAccount {
    pubkey: String,
    #[serde(rename = "isSigner")]
    is_signer: bool,
    #[serde(rename = "isWritable")]
    is_writable: bool,
}

fn parse_address(s: &str) -> Address {
    let bytes = bs58::decode(s).into_vec().expect("invalid base58");
    let arr: [u8; 32] = bytes.try_into().expect("not 32 bytes");
    Address::from(arr)
}

/// Fetch a Jupiter swap route via the public API.
///
/// `user_pubkey` should be the pool PDA (the account that signs the CPI).
pub fn fetch_jupiter_route(
    in_mint: &Address,
    out_mint: &Address,
    amount: u64,
    slippage_bps: u16,
    user_pubkey: &Address,
) -> Result<JupiterRoute, String> {
    let in_mint_str = bs58::encode(in_mint.as_ref()).into_string();
    let out_mint_str = bs58::encode(out_mint.as_ref()).into_string();
    let user_str = bs58::encode(user_pubkey.as_ref()).into_string();

    // Step 1: Quote
    let quote_url = format!(
        "https://api.jup.ag/swap/v1/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}",
        in_mint_str, out_mint_str, amount, slippage_bps
    );
    let quote_body: String = ureq::get(&quote_url)
        .call()
        .map_err(|e| format!("quote request failed: {}", e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("quote read failed: {}", e))?;

    let quote: QuoteResponse = serde_json::from_str(&quote_body).map_err(|e| {
        format!(
            "quote parse failed: {} body: {}",
            e,
            &quote_body[..200.min(quote_body.len())]
        )
    })?;

    // Step 2: Swap instructions
    let swap_body = serde_json::json!({
        "quoteResponse": serde_json::from_str::<serde_json::Value>(&quote_body).unwrap(),
        "userPublicKey": user_str,
        "wrapAndUnwrapSol": false,
        "asLegacyTransaction": true,
    });

    let swap_resp: String = ureq::post("https://api.jup.ag/swap/v1/swap-instructions")
        .header("Content-Type", "application/json")
        .send(swap_body.to_string().as_bytes())
        .map_err(|e| format!("swap-instructions request failed: {}", e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("swap-instructions read failed: {}", e))?;

    let si: SwapInstructionsResponse = serde_json::from_str(&swap_resp).map_err(|e| {
        format!(
            "swap-instructions parse failed: {} body: {}",
            e,
            &swap_resp[..300.min(swap_resp.len())]
        )
    })?;

    // Decode instruction data
    use base64::Engine;
    let swap_data = base64::engine::general_purpose::STANDARD
        .decode(&si.swap_instruction.data)
        .map_err(|e| format!("base64 decode failed: {}", e))?;

    let accounts: Vec<JupiterAccount> = si
        .swap_instruction
        .accounts
        .iter()
        .map(|a| JupiterAccount {
            pubkey: parse_address(&a.pubkey),
            is_signer: a.is_signer,
            is_writable: a.is_writable,
        })
        .collect();

    let alts: Vec<Address> = si
        .address_lookup_table_addresses
        .iter()
        .map(|s| parse_address(s))
        .collect();

    Ok(JupiterRoute {
        swap_data,
        accounts,
        address_lookup_tables: alts,
        in_amount: quote.in_amount.parse().unwrap_or(amount),
        out_amount: quote.out_amount.parse().unwrap_or(0),
    })
}

/// Clone all accounts referenced in a Jupiter route from mainnet.
///
/// Writable accounts get extra data padding (10KB) to support realloc
/// during Jupiter's CPI. We also detect and clone any program accounts
/// (executables) that the route references, including their programdata
/// accounts for BPF Upgradeable Loader programs.
pub fn fork_jupiter_state(svm: &mut LiteSVM, rpc: &RpcClient, route: &JupiterRoute) -> usize {
    let mut total = 0;
    let mut program_addrs = Vec::new();

    for ja in &route.accounts {
        // Skip system programs and known builtins (already in LiteSVM)
        if is_builtin(&ja.pubkey) {
            continue;
        }

        // Already loaded (e.g. our own program)? Skip.
        if let Some(existing) = svm.get_account(&ja.pubkey) {
            if existing.executable {
                program_addrs.push(ja.pubkey);
            }
            // Still pad writable accounts that already exist
            if ja.is_writable && existing.data.len() < 165 + MAX_PERMITTED_DATA_INCREASE {
                let mut padded = existing.clone();
                padded.data.resize(padded.data.len() + MAX_PERMITTED_DATA_INCREASE, 0);
                svm.set_account(ja.pubkey, padded).unwrap();
            }
            continue;
        }

        // Clone from mainnet — pad ALL accounts (not just writable) because
        // Jupiter may CPI into DEX programs that realloc their own accounts
        if clone_with_realloc_padding(svm, rpc, &ja.pubkey, MAX_PERMITTED_DATA_INCREASE) {
            total += 1;
            if let Some(acc) = svm.get_account(&ja.pubkey) {
                if acc.executable {
                    program_addrs.push(ja.pubkey);
                }
            }
        }
    }

    // Clone address lookup tables
    for alt in &route.address_lookup_tables {
        if clone_from_rpc(svm, rpc, alt) {
            total += 1;
        }
    }

    // For each program discovered in the route, clone its programdata account
    // (BPF Upgradeable Loader stores the actual bytecode in a separate account)
    for prog_addr in &program_addrs {
        if let Some(acc) = svm.get_account(prog_addr) {
            // BPF Upgradeable Loader: first 4 bytes = account type (2 = Program),
            // next 32 bytes = programdata address
            if acc.data.len() >= 36 {
                let tag = u32::from_le_bytes(acc.data[0..4].try_into().unwrap_or([0; 4]));
                if tag == 2 {
                    // tag 2 = Program account, next 32 bytes = programdata
                    let pd_bytes: [u8; 32] = acc.data[4..36].try_into().unwrap();
                    let pd_addr = Address::from(pd_bytes);
                    // Programdata can be large (megabytes), clone it
                    if clone_from_rpc(svm, rpc, &pd_addr) {
                        total += 1;
                    }
                }
            }
        }
    }

    total
}

/// Check if an address is a known builtin program (already loaded in LiteSVM).
fn is_builtin(addr: &Address) -> bool {
    let bytes = addr.as_ref();
    // System Program (all zeros)
    if bytes == &[0u8; 32] { return true; }
    // Check known program IDs by their last byte pattern (fast heuristic)
    // Token Program: TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
    if bytes[31] == 0xa9 && bytes[0] == 0x06 { return true; }
    // Sysvar Clock, Instructions, etc. (11111111...)
    if bytes.iter().all(|&b| b == 0x01 || b == 0x00) { return true; }
    false
}
