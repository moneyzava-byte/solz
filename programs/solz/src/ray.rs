//! Raydium CPMM, called directly by instruction. Account order and data follow
//! the published raydium-cp-swap program exactly.
use crate::constants::*;
use crate::errors::SolzError;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke_signed;

const IX_INITIALIZE: [u8; 8] = [175, 175, 109, 31, 13, 152, 155, 237];
const IX_SWAP_BASE_INPUT: [u8; 8] = [143, 190, 90, 218, 196, 30, 51, 222];
const IX_DEPOSIT: [u8; 8] = [242, 35, 198, 137, 82, 225, 242, 182];
const POOL_STATE_DISC: [u8; 8] = [247, 237, 227, 245, 215, 195, 222, 70];
const AMM_CONFIG_DISC: [u8; 8] = [218, 244, 33, 104, 203, 203, 43, 111];

// PoolState is repr(C, packed): 8 discriminator + 10 pubkeys, then five u8.
const OFF_TOKEN_0_VAULT: usize = 8 + 32 * 2;
const OFF_TOKEN_1_VAULT: usize = 8 + 32 * 3;
const OFF_LP_MINT: usize = 8 + 32 * 4;
const OFF_TOKEN_0_MINT: usize = 8 + 32 * 5;
const OFF_TOKEN_1_MINT: usize = 8 + 32 * 6;
const OFF_LP_SUPPLY: usize = 8 + 320 + 5;
const OFF_PROTOCOL_0: usize = OFF_LP_SUPPLY + 8;
const OFF_PROTOCOL_1: usize = OFF_LP_SUPPLY + 16;
const OFF_FUND_0: usize = OFF_LP_SUPPLY + 24;
const OFF_FUND_1: usize = OFF_LP_SUPPLY + 32;
const OFF_CREATOR_0: usize = OFF_LP_SUPPLY + 64;
const OFF_CREATOR_1: usize = OFF_LP_SUPPLY + 72;
// AmmConfig: 8 discriminator, bump u8, disable bool, index u16, then trade_fee_rate u64.
const OFF_TRADE_FEE: usize = 12;

fn rd_u64(d: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(d[off..off + 8].try_into().unwrap())
}
fn rd_key(d: &[u8], off: usize) -> Pubkey {
    Pubkey::new_from_array(d[off..off + 32].try_into().unwrap())
}

pub fn amm_config_key() -> Pubkey {
    Pubkey::find_program_address(&[RAY_AMM_CONFIG_SEED, &RAY_AMM_CONFIG_INDEX.to_be_bytes()], &RAYDIUM_CPMM).0
}
pub fn authority_key() -> Pubkey {
    Pubkey::find_program_address(&[RAY_AUTH_SEED], &RAYDIUM_CPMM).0
}

pub struct PoolView {
    pub lp_supply: u64,
    pub sol_reserve: u64,
    pub token_reserve: u64,
}

/// Reads the pool's reserves the way Raydium prices them: vault balances less
/// the protocol, fund and creator fees the vaults are still holding.
pub fn view(
    pool: &AccountInfo,
    token_vault: &AccountInfo,
    sol_vault: &AccountInfo,
    token_is_0: bool,
) -> Result<PoolView> {
    require_keys_eq!(*pool.owner, RAYDIUM_CPMM, SolzError::WrongAccount);
    let d = pool.try_borrow_data()?;
    require!(d.len() >= OFF_CREATOR_1 + 8 && d[..8] == POOL_STATE_DISC, SolzError::WrongAccount);
    let (tv_off, sv_off) = if token_is_0 { (OFF_TOKEN_0_VAULT, OFF_TOKEN_1_VAULT) } else { (OFF_TOKEN_1_VAULT, OFF_TOKEN_0_VAULT) };
    require_keys_eq!(rd_key(&d, tv_off), token_vault.key(), SolzError::WrongAccount);
    require_keys_eq!(rd_key(&d, sv_off), sol_vault.key(), SolzError::WrongAccount);
    let fees0 = rd_u64(&d, OFF_PROTOCOL_0) + rd_u64(&d, OFF_FUND_0) + rd_u64(&d, OFF_CREATOR_0);
    let fees1 = rd_u64(&d, OFF_PROTOCOL_1) + rd_u64(&d, OFF_FUND_1) + rd_u64(&d, OFF_CREATOR_1);
    let (tf, sf) = if token_is_0 { (fees0, fees1) } else { (fees1, fees0) };
    let tok_bal = token_amount(token_vault)?;
    let sol_bal = token_amount(sol_vault)?;
    Ok(PoolView {
        lp_supply: rd_u64(&d, OFF_LP_SUPPLY),
        sol_reserve: sol_bal.checked_sub(sf).ok_or(SolzError::WrongAccount)?,
        token_reserve: tok_bal.checked_sub(tf).ok_or(SolzError::WrongAccount)?,
    })
}

pub fn pool_keys(pool: &AccountInfo) -> Result<(Pubkey, Pubkey, Pubkey, Pubkey, Pubkey)> {
    let d = pool.try_borrow_data()?;
    Ok((
        rd_key(&d, OFF_TOKEN_0_VAULT),
        rd_key(&d, OFF_TOKEN_1_VAULT),
        rd_key(&d, OFF_LP_MINT),
        rd_key(&d, OFF_TOKEN_0_MINT),
        rd_key(&d, OFF_TOKEN_1_MINT),
    ))
}

pub fn trade_fee_rate(amm_config: &AccountInfo) -> Result<u64> {
    require_keys_eq!(*amm_config.owner, RAYDIUM_CPMM, SolzError::WrongAccount);
    require_keys_eq!(amm_config.key(), amm_config_key(), SolzError::WrongAccount);
    let d = amm_config.try_borrow_data()?;
    require!(d.len() >= OFF_TRADE_FEE + 8 && d[..8] == AMM_CONFIG_DISC, SolzError::WrongAccount);
    Ok(rd_u64(&d, OFF_TRADE_FEE))
}

/// Raydium's switch that stops new pools on a fee tier.
pub fn create_disabled(amm_config: &AccountInfo) -> Result<bool> {
    trade_fee_rate(amm_config)?; // checks owner, address and discriminator
    let d = amm_config.try_borrow_data()?;
    Ok(d[9] != 0)
}

/// Raydium's pool switches: bit 0 deposit, bit 1 withdraw, bit 2 swap; a set bit is off.
pub fn status(pool: &AccountInfo) -> Result<u8> {
    require_keys_eq!(*pool.owner, RAYDIUM_CPMM, SolzError::WrongAccount);
    let d = pool.try_borrow_data()?;
    require!(d.len() > 8 + 320 + 1 && d[..8] == POOL_STATE_DISC, SolzError::WrongAccount);
    Ok(d[8 + 320 + 1])
}

/// Amount field of an SPL Token or Token-2022 account (same base layout).
pub fn token_amount(acc: &AccountInfo) -> Result<u64> {
    let d = acc.try_borrow_data()?;
    require!(d.len() >= 72, SolzError::WrongAccount);
    require!(
        *acc.owner == anchor_spl::token::ID || *acc.owner == anchor_spl::token_2022::ID,
        SolzError::WrongAccount
    );
    Ok(rd_u64(&d, 64))
}

pub struct InitAccounts<'a, 'info> {
    pub creator: &'a AccountInfo<'info>,
    pub amm_config: &'a AccountInfo<'info>,
    pub authority: &'a AccountInfo<'info>,
    pub pool_state: &'a AccountInfo<'info>,
    pub token_0_mint: &'a AccountInfo<'info>,
    pub token_1_mint: &'a AccountInfo<'info>,
    pub lp_mint: &'a AccountInfo<'info>,
    pub creator_token_0: &'a AccountInfo<'info>,
    pub creator_token_1: &'a AccountInfo<'info>,
    pub creator_lp_token: &'a AccountInfo<'info>,
    pub token_0_vault: &'a AccountInfo<'info>,
    pub token_1_vault: &'a AccountInfo<'info>,
    pub create_pool_fee: &'a AccountInfo<'info>,
    pub observation_state: &'a AccountInfo<'info>,
    pub token_program: &'a AccountInfo<'info>,
    pub token_0_program: &'a AccountInfo<'info>,
    pub token_1_program: &'a AccountInfo<'info>,
    pub associated_token_program: &'a AccountInfo<'info>,
    pub system_program: &'a AccountInfo<'info>,
    pub rent: &'a AccountInfo<'info>,
    pub raydium: &'a AccountInfo<'info>,
}

pub fn initialize(a: InitAccounts, amount_0: u64, amount_1: u64, seeds: &[&[&[u8]]]) -> Result<()> {
    let metas = vec![
        AccountMeta::new(a.creator.key(), true),
        AccountMeta::new_readonly(a.amm_config.key(), false),
        AccountMeta::new_readonly(a.authority.key(), false),
        AccountMeta::new(a.pool_state.key(), false),
        AccountMeta::new_readonly(a.token_0_mint.key(), false),
        AccountMeta::new_readonly(a.token_1_mint.key(), false),
        AccountMeta::new(a.lp_mint.key(), false),
        AccountMeta::new(a.creator_token_0.key(), false),
        AccountMeta::new(a.creator_token_1.key(), false),
        AccountMeta::new(a.creator_lp_token.key(), false),
        AccountMeta::new(a.token_0_vault.key(), false),
        AccountMeta::new(a.token_1_vault.key(), false),
        AccountMeta::new(a.create_pool_fee.key(), false),
        AccountMeta::new(a.observation_state.key(), false),
        AccountMeta::new_readonly(a.token_program.key(), false),
        AccountMeta::new_readonly(a.token_0_program.key(), false),
        AccountMeta::new_readonly(a.token_1_program.key(), false),
        AccountMeta::new_readonly(a.associated_token_program.key(), false),
        AccountMeta::new_readonly(a.system_program.key(), false),
        AccountMeta::new_readonly(a.rent.key(), false),
    ];
    let mut data = IX_INITIALIZE.to_vec();
    data.extend_from_slice(&amount_0.to_le_bytes());
    data.extend_from_slice(&amount_1.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // open now
    let ix = Instruction { program_id: RAYDIUM_CPMM, accounts: metas, data };
    invoke_signed(
        &ix,
        &[
            a.creator.clone(), a.amm_config.clone(), a.authority.clone(), a.pool_state.clone(),
            a.token_0_mint.clone(), a.token_1_mint.clone(), a.lp_mint.clone(), a.creator_token_0.clone(),
            a.creator_token_1.clone(), a.creator_lp_token.clone(), a.token_0_vault.clone(),
            a.token_1_vault.clone(), a.create_pool_fee.clone(), a.observation_state.clone(),
            a.token_program.clone(), a.token_0_program.clone(), a.token_1_program.clone(),
            a.associated_token_program.clone(), a.system_program.clone(), a.rent.clone(), a.raydium.clone(),
        ],
        seeds,
    )?;
    Ok(())
}

pub struct SwapAccounts<'a, 'info> {
    pub payer: &'a AccountInfo<'info>,
    pub authority: &'a AccountInfo<'info>,
    pub amm_config: &'a AccountInfo<'info>,
    pub pool_state: &'a AccountInfo<'info>,
    pub input_token_account: &'a AccountInfo<'info>,
    pub output_token_account: &'a AccountInfo<'info>,
    pub input_vault: &'a AccountInfo<'info>,
    pub output_vault: &'a AccountInfo<'info>,
    pub input_token_program: &'a AccountInfo<'info>,
    pub output_token_program: &'a AccountInfo<'info>,
    pub input_token_mint: &'a AccountInfo<'info>,
    pub output_token_mint: &'a AccountInfo<'info>,
    pub observation_state: &'a AccountInfo<'info>,
    pub raydium: &'a AccountInfo<'info>,
}

pub fn swap_base_input(a: SwapAccounts, amount_in: u64, min_out: u64, seeds: &[&[&[u8]]]) -> Result<()> {
    let metas = vec![
        AccountMeta::new_readonly(a.payer.key(), true),
        AccountMeta::new_readonly(a.authority.key(), false),
        AccountMeta::new_readonly(a.amm_config.key(), false),
        AccountMeta::new(a.pool_state.key(), false),
        AccountMeta::new(a.input_token_account.key(), false),
        AccountMeta::new(a.output_token_account.key(), false),
        AccountMeta::new(a.input_vault.key(), false),
        AccountMeta::new(a.output_vault.key(), false),
        AccountMeta::new_readonly(a.input_token_program.key(), false),
        AccountMeta::new_readonly(a.output_token_program.key(), false),
        AccountMeta::new_readonly(a.input_token_mint.key(), false),
        AccountMeta::new_readonly(a.output_token_mint.key(), false),
        AccountMeta::new(a.observation_state.key(), false),
    ];
    let mut data = IX_SWAP_BASE_INPUT.to_vec();
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&min_out.to_le_bytes());
    let ix = Instruction { program_id: RAYDIUM_CPMM, accounts: metas, data };
    invoke_signed(
        &ix,
        &[
            a.payer.clone(), a.authority.clone(), a.amm_config.clone(), a.pool_state.clone(),
            a.input_token_account.clone(), a.output_token_account.clone(), a.input_vault.clone(),
            a.output_vault.clone(), a.input_token_program.clone(), a.output_token_program.clone(),
            a.input_token_mint.clone(), a.output_token_mint.clone(), a.observation_state.clone(), a.raydium.clone(),
        ],
        seeds,
    )?;
    Ok(())
}

pub struct DepositAccounts<'a, 'info> {
    pub owner: &'a AccountInfo<'info>,
    pub authority: &'a AccountInfo<'info>,
    pub pool_state: &'a AccountInfo<'info>,
    pub owner_lp_token: &'a AccountInfo<'info>,
    pub token_0_account: &'a AccountInfo<'info>,
    pub token_1_account: &'a AccountInfo<'info>,
    pub token_0_vault: &'a AccountInfo<'info>,
    pub token_1_vault: &'a AccountInfo<'info>,
    pub token_program: &'a AccountInfo<'info>,
    pub token_program_2022: &'a AccountInfo<'info>,
    pub vault_0_mint: &'a AccountInfo<'info>,
    pub vault_1_mint: &'a AccountInfo<'info>,
    pub lp_mint: &'a AccountInfo<'info>,
    pub raydium: &'a AccountInfo<'info>,
}

pub fn deposit(a: DepositAccounts, lp: u64, max_0: u64, max_1: u64, seeds: &[&[&[u8]]]) -> Result<()> {
    let metas = vec![
        AccountMeta::new_readonly(a.owner.key(), true),
        AccountMeta::new_readonly(a.authority.key(), false),
        AccountMeta::new(a.pool_state.key(), false),
        AccountMeta::new(a.owner_lp_token.key(), false),
        AccountMeta::new(a.token_0_account.key(), false),
        AccountMeta::new(a.token_1_account.key(), false),
        AccountMeta::new(a.token_0_vault.key(), false),
        AccountMeta::new(a.token_1_vault.key(), false),
        AccountMeta::new_readonly(a.token_program.key(), false),
        AccountMeta::new_readonly(a.token_program_2022.key(), false),
        AccountMeta::new_readonly(a.vault_0_mint.key(), false),
        AccountMeta::new_readonly(a.vault_1_mint.key(), false),
        AccountMeta::new(a.lp_mint.key(), false),
    ];
    let mut data = IX_DEPOSIT.to_vec();
    data.extend_from_slice(&lp.to_le_bytes());
    data.extend_from_slice(&max_0.to_le_bytes());
    data.extend_from_slice(&max_1.to_le_bytes());
    let ix = Instruction { program_id: RAYDIUM_CPMM, accounts: metas, data };
    invoke_signed(
        &ix,
        &[
            a.owner.clone(), a.authority.clone(), a.pool_state.clone(), a.owner_lp_token.clone(),
            a.token_0_account.clone(), a.token_1_account.clone(), a.token_0_vault.clone(),
            a.token_1_vault.clone(), a.token_program.clone(), a.token_program_2022.clone(),
            a.vault_0_mint.clone(), a.vault_1_mint.clone(), a.lp_mint.clone(), a.raydium.clone(),
        ],
        seeds,
    )?;
    Ok(())
}
