use litesvm::LiteSVM;
use serde::Deserialize;
use solana_address::Address;
use solana_rpc_client::rpc_client::RpcClient;

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
pub fn fork_jupiter_state(svm: &mut LiteSVM, rpc: &RpcClient, route: &JupiterRoute) -> usize {
    let addrs: Vec<Address> = route.accounts.iter().map(|a| a.pubkey).collect();
    let mut total = clone_accounts_batch(svm, rpc, &addrs);
    total += clone_accounts_batch(svm, rpc, &route.address_lookup_tables);
    total
}
