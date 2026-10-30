//! Local stand-in for Raydium CPMM, used only by the test validator. It sits
//! at Raydium's program id and matches the instruction names (so the Anchor
//! discriminators), account order and the PoolState / AmmConfig byte layout
//! the SOLZ program reads. Pricing is a constant product with the config's
//! trade fee; 12 percent of each fee is held back as a protocol fee, as on
//! Raydium, so fee accounting in the reserves is exercised too.
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

declare_id!("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");

pub const AUTH_SEED: &[u8] = b"vault_and_lp_mint_auth_seed";

#[program]
pub mod raymock {
    use super::*;

    pub fn create_amm_config(ctx: Context<CreateAmmConfig>, index: u16, trade_fee_rate: u64, create_pool_fee: u64) -> Result<()> {
        let c = &mut ctx.accounts.amm_config;
        c.bump = ctx.bumps.amm_config;
        c.index = index;
        c.trade_fee_rate = trade_fee_rate;
        c.protocol_fee_rate = 120_000;
        c.create_pool_fee = create_pool_fee;
        Ok(())
    }

    pub fn initialize(ctx: Context<Initialize>, init_amount_0: u64, init_amount_1: u64, _open_time: u64) -> Result<()> {
        let a = &ctx.accounts;
        require!(a.token_0_mint.key() < a.token_1_mint.key(), MockError::Order);
        xfer(&a.token_0_program, &a.creator_token_0, &a.token_0_mint, &a.token_0_vault, &a.creator.to_account_info(), init_amount_0, &[])?;
        xfer(&a.token_1_program, &a.creator_token_1, &a.token_1_mint, &a.token_1_vault, &a.creator.to_account_info(), init_amount_1, &[])?;
        let liquidity = isqrt((init_amount_0 as u128) * (init_amount_1 as u128)) as u64;
        let auth_seeds: &[&[u8]] = &[AUTH_SEED, &[ctx.bumps.authority]];
        anchor_spl::token::mint_to(
            CpiContext::new_with_signer(
                a.token_program.key(),
                anchor_spl::token::MintTo {
                    mint: a.lp_mint.to_account_info(),
                    to: a.creator_lp_token.to_account_info(),
                    authority: a.authority.to_account_info(),
                },
                &[auth_seeds],
            ),
            liquidity - 100,
        )?;
        if a.amm_config.create_pool_fee > 0 {
            anchor_lang::system_program::transfer(
                CpiContext::new(
                    a.system_program.key(),
                    anchor_lang::system_program::Transfer { from: a.creator.to_account_info(), to: a.create_pool_fee.to_account_info() },
                ),
                a.amm_config.create_pool_fee,
            )?;
        }
        let mut p = ctx.accounts.pool_state.load_init()?;
        p.amm_config = a.amm_config.key();
        p.pool_creator = a.creator.key();
        p.token_0_vault = a.token_0_vault.key();
        p.token_1_vault = a.token_1_vault.key();
        p.lp_mint = a.lp_mint.key();
        p.token_0_mint = a.token_0_mint.key();
        p.token_1_mint = a.token_1_mint.key();
        p.token_0_program = a.token_0_program.key();
        p.token_1_program = a.token_1_program.key();
        p.observation_key = a.observation_state.key();
        p.auth_bump = ctx.bumps.authority;
        p.lp_mint_decimals = 9;
        p.mint_0_decimals = a.token_0_mint.decimals;
        p.mint_1_decimals = a.token_1_mint.decimals;
        p.lp_supply = liquidity;
        Ok(())
    }

    pub fn swap_base_input(ctx: Context<Swap>, amount_in: u64, minimum_amount_out: u64) -> Result<()> {
        let a = &ctx.accounts;
        let (zero_in, r_in, r_out) = {
            let p = a.pool_state.load()?;
            let zero_in = a.input_vault.key() == p.token_0_vault;
            require!(zero_in || a.input_vault.key() == p.token_1_vault, MockError::Vault);
            let (f0, f1) = (p.protocol_fees_token_0, p.protocol_fees_token_1);
            let (r0, r1) = vault_amt(&a.input_vault, &a.output_vault, zero_in);
            let (fi, fo) = if zero_in { (f0, f1) } else { (f1, f0) };
            (zero_in, r0 - fi, r1 - fo)
        };
        let fee = ((amount_in as u128) * (a.amm_config.trade_fee_rate as u128)).div_ceil(1_000_000) as u64;
        let protocol = ((fee as u128) * (a.amm_config.protocol_fee_rate as u128) / 1_000_000) as u64;
        let in_less = amount_in - fee;
        let out = ((in_less as u128) * (r_out as u128) / ((r_in as u128) + (in_less as u128))) as u64;
        require!(out >= minimum_amount_out && out > 0, MockError::Slippage);
        xfer(&a.input_token_program, &a.input_token_account, &a.input_token_mint, &a.input_vault, &a.payer.to_account_info(), amount_in, &[])?;
        let auth_seeds: &[&[u8]] = &[AUTH_SEED, &[ctx.bumps.authority]];
        xfer(&a.output_token_program, &a.output_vault, &a.output_token_mint, &a.output_token_account, &a.authority.to_account_info(), out, &[auth_seeds])?;
        let mut p = a.pool_state.load_mut()?;
        if zero_in {
            p.protocol_fees_token_0 += protocol;
        } else {
            p.protocol_fees_token_1 += protocol;
        }
        Ok(())
    }

    pub fn deposit(ctx: Context<Deposit>, lp_token_amount: u64, maximum_token_0_amount: u64, maximum_token_1_amount: u64) -> Result<()> {
        let a = &ctx.accounts;
        let (supply, t0, t1) = {
            let p = a.pool_state.load()?;
            require!(a.token_0_vault.key() == p.token_0_vault && a.token_1_vault.key() == p.token_1_vault, MockError::Vault);
            (p.lp_supply, a.token_0_vault.amount - p.protocol_fees_token_0, a.token_1_vault.amount - p.protocol_fees_token_1)
        };
        let need0 = ((lp_token_amount as u128) * (t0 as u128)).div_ceil(supply as u128) as u64;
        let need1 = ((lp_token_amount as u128) * (t1 as u128)).div_ceil(supply as u128) as u64;
        require!(need0 <= maximum_token_0_amount && need1 <= maximum_token_1_amount, MockError::Slippage);
        let p0 = if a.vault_0_mint.to_account_info().owner == &anchor_spl::token::ID { a.token_program.to_account_info() } else { a.token_program_2022.to_account_info() };
        let p1 = if a.vault_1_mint.to_account_info().owner == &anchor_spl::token::ID { a.token_program.to_account_info() } else { a.token_program_2022.to_account_info() };
        xfer_raw(&p0, &a.token_0_account, &a.vault_0_mint, &a.token_0_vault, &a.owner.to_account_info(), need0, &[])?;
        xfer_raw(&p1, &a.token_1_account, &a.vault_1_mint, &a.token_1_vault, &a.owner.to_account_info(), need1, &[])?;
        let auth_seeds: &[&[u8]] = &[AUTH_SEED, &[ctx.bumps.authority]];
        anchor_spl::token::mint_to(
            CpiContext::new_with_signer(
                a.token_program.key(),
                anchor_spl::token::MintTo {
                    mint: a.lp_mint.to_account_info(),
                    to: a.owner_lp_token.to_account_info(),
                    authority: a.authority.to_account_info(),
                },
                &[auth_seeds],
            ),
            lp_token_amount,
        )?;
        a.pool_state.load_mut()?.lp_supply += lp_token_amount;
        Ok(())
    }
}

fn vault_amt(i: &InterfaceAccount<TokenAccount>, o: &InterfaceAccount<TokenAccount>, _zero_in: bool) -> (u64, u64) {
    (i.amount, o.amount)
}

fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

fn xfer<'info>(
    program: &Interface<'info, TokenInterface>,
    from: &InterfaceAccount<'info, TokenAccount>,
    mint: &InterfaceAccount<'info, Mint>,
    to: &InterfaceAccount<'info, TokenAccount>,
    authority: &AccountInfo<'info>,
    amount: u64,
    seeds: &[&[&[u8]]],
) -> Result<()> {
    xfer_raw(&program.to_account_info(), from, mint, to, authority, amount, seeds)
}

fn xfer_raw<'info>(
    program: &AccountInfo<'info>,
    from: &InterfaceAccount<'info, TokenAccount>,
    mint: &InterfaceAccount<'info, Mint>,
    to: &InterfaceAccount<'info, TokenAccount>,
    authority: &AccountInfo<'info>,
    amount: u64,
    seeds: &[&[&[u8]]],
) -> Result<()> {
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            program.key(),
            TransferChecked {
                from: from.to_account_info(),
                mint: mint.to_account_info(),
                to: to.to_account_info(),
                authority: authority.clone(),
            },
            seeds,
        ),
        amount,
        mint.decimals,
    )
}

#[error_code]
pub enum MockError {
    Order,
    Vault,
    Slippage,
}

#[account]
#[derive(Default)]
pub struct AmmConfig {
    pub bump: u8,
    pub disable_create_pool: bool,
    pub index: u16,
    pub trade_fee_rate: u64,
    pub protocol_fee_rate: u64,
    pub fund_fee_rate: u64,
    pub create_pool_fee: u64,
    pub protocol_owner: Pubkey,
    pub fund_owner: Pubkey,
    pub creator_fee_rate: u64,
    pub creator_fee_share_rate: u64,
    pub padding: [u64; 14],
}

#[account(zero_copy(unsafe))]
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct PoolState {
    pub amm_config: Pubkey,
    pub pool_creator: Pubkey,
    pub token_0_vault: Pubkey,
    pub token_1_vault: Pubkey,
    pub lp_mint: Pubkey,
    pub token_0_mint: Pubkey,
    pub token_1_mint: Pubkey,
    pub token_0_program: Pubkey,
    pub token_1_program: Pubkey,
    pub observation_key: Pubkey,
    pub auth_bump: u8,
    pub status: u8,
    pub lp_mint_decimals: u8,
    pub mint_0_decimals: u8,
    pub mint_1_decimals: u8,
    pub lp_supply: u64,
    pub protocol_fees_token_0: u64,
    pub protocol_fees_token_1: u64,
    pub fund_fees_token_0: u64,
    pub fund_fees_token_1: u64,
    pub open_time: u64,
    pub recent_epoch: u64,
    pub creator_fee_on: u8,
    pub enable_creator_fee: bool,
    pub padding1: [u8; 6],
    pub creator_fees_token_0: u64,
    pub creator_fees_token_1: u64,
    pub padding: [u64; 28],
}

#[account]
pub struct ObservationState {
    pub pool_id: Pubkey,
}

#[derive(Accounts)]
#[instruction(index: u16)]
pub struct CreateAmmConfig<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(init, payer = owner, space = 8 + 1 + 1 + 2 + 32 + 64 + 16 + 112, seeds = [b"amm_config".as_ref(), index.to_be_bytes().as_ref()], bump)]
    pub amm_config: Account<'info, AmmConfig>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    pub amm_config: Box<Account<'info, AmmConfig>>,
    /// CHECK: pda
    #[account(seeds = [AUTH_SEED], bump)]
    pub authority: UncheckedAccount<'info>,
    #[account(
        init,
        payer = creator,
        space = 8 + std::mem::size_of::<PoolState>(),
        seeds = [b"pool", amm_config.key().as_ref(), token_0_mint.key().as_ref(), token_1_mint.key().as_ref()],
        bump
    )]
    pub pool_state: AccountLoader<'info, PoolState>,
    #[account(mint::token_program = token_0_program)]
    pub token_0_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mint::token_program = token_1_program)]
    pub token_1_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        init,
        seeds = [b"pool_lp_mint", pool_state.key().as_ref()],
        bump,
        mint::decimals = 9,
        mint::authority = authority,
        payer = creator,
        mint::token_program = token_program,
    )]
    pub lp_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, token::mint = token_0_mint, token::authority = creator)]
    pub creator_token_0: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = token_1_mint, token::authority = creator)]
    pub creator_token_1: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init,
        associated_token::mint = lp_mint,
        associated_token::authority = creator,
        payer = creator,
        associated_token::token_program = token_program,
    )]
    pub creator_lp_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init,
        payer = creator,
        seeds = [b"pool_vault", pool_state.key().as_ref(), token_0_mint.key().as_ref()],
        bump,
        token::mint = token_0_mint,
        token::authority = authority,
        token::token_program = token_0_program,
    )]
    pub token_0_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init,
        payer = creator,
        seeds = [b"pool_vault", pool_state.key().as_ref(), token_1_mint.key().as_ref()],
        bump,
        token::mint = token_1_mint,
        token::authority = authority,
        token::token_program = token_1_program,
    )]
    pub token_1_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: fee receiver
    #[account(mut)]
    pub create_pool_fee: UncheckedAccount<'info>,
    #[account(init, seeds = [b"observation", pool_state.key().as_ref()], bump, payer = creator, space = 8 + 32)]
    pub observation_state: Box<Account<'info, ObservationState>>,
    pub token_program: Program<'info, Token>,
    pub token_0_program: Interface<'info, TokenInterface>,
    pub token_1_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Swap<'info> {
    pub payer: Signer<'info>,
    /// CHECK: pda
    #[account(seeds = [AUTH_SEED], bump)]
    pub authority: UncheckedAccount<'info>,
    pub amm_config: Box<Account<'info, AmmConfig>>,
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
    #[account(mut)]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub input_token_program: Interface<'info, TokenInterface>,
    pub output_token_program: Interface<'info, TokenInterface>,
    pub input_token_mint: Box<InterfaceAccount<'info, Mint>>,
    pub output_token_mint: Box<InterfaceAccount<'info, Mint>>,
    /// CHECK: observation
    #[account(mut)]
    pub observation_state: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    pub owner: Signer<'info>,
    /// CHECK: pda
    #[account(seeds = [AUTH_SEED], bump)]
    pub authority: UncheckedAccount<'info>,
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
    #[account(mut)]
    pub owner_lp_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub token_0_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub token_1_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub token_0_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub token_1_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub token_program_2022: Program<'info, anchor_spl::token_2022::Token2022>,
    pub vault_0_mint: Box<InterfaceAccount<'info, Mint>>,
    pub vault_1_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut)]
    pub lp_mint: Box<InterfaceAccount<'info, Mint>>,
}
