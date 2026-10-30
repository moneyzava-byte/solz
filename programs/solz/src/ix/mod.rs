pub mod admin;
pub mod keepers;
pub mod launch;
pub mod mining;
pub mod staking;
pub mod util;

use crate::constants::*;
use crate::errors::SolzError;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::Token;
use anchor_spl::token_2022::Token2022;
use anchor_spl::token_interface::{Mint, TokenAccount};
use util::*;

// ------------------------------------------------------------------ admin

#[derive(Accounts)]
pub struct InitGlobal<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(init, payer = payer, space = 8 + Global::INIT_SPACE, seeds = [b"global"], bump)]
    pub global: Account<'info, Global>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct WithdrawTo<'info> {
    pub caller: Signer<'info>,
    #[account(mut, seeds = [MINE_SEED, mine.mint.as_ref()], bump = mine.bump)]
    pub mine: Box<Account<'info, Mine>>,
    /// CHECK: the mine's vault PDA
    #[account(mut, seeds = [VAULT_SEED, mine.key().as_ref()], bump = mine.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: must be the operations wallet or the mine's creator; checked in the handler.
    #[account(mut)]
    pub to: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

// ------------------------------------------------------------------ launch

#[derive(Accounts)]
#[instruction(kind: u8, name: String, symbol: String)]
pub struct CreateMine<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut, seeds = [b"global"], bump = global.bump)]
    pub global: Box<Account<'info, Global>>,
    #[account(
        init,
        payer = creator,
        space = 8 + Symbol::INIT_SPACE,
        seeds = [b"sym", symbol.as_bytes()],
        bump
    )]
    pub symbol_entry: Box<Account<'info, Symbol>>,
    /// CHECK: created in the handler as a Token-2022 mint with metadata.
    #[account(mut, seeds = [b"mint", symbol.as_bytes()], bump)]
    pub mint: UncheckedAccount<'info>,
    #[account(init, payer = creator, space = 8 + Mine::INIT_SPACE, seeds = [MINE_SEED, mint.key().as_ref()], bump)]
    pub mine: Box<Account<'info, Mine>>,
    /// CHECK: the mine's vault, a system account PDA funded with rent here.
    #[account(mut, seeds = [VAULT_SEED, mine.key().as_ref()], bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: program authority for every mint and its metadata.
    #[account(seeds = [AUTH_SEED], bump)]
    pub auth: UncheckedAccount<'info>,
    /// $SOLZ's mine and vault, which take the launch fee. Absent for $SOLZ itself.
    #[account(mut)]
    pub solz_mine: Option<Box<Account<'info, Mine>>>,
    /// CHECK: $SOLZ's vault, checked against solz_mine in the handler.
    #[account(mut)]
    pub solz_vault: Option<UncheckedAccount<'info>>,
    /// CHECK: Raydium's fee tier, read to make sure it takes new pools
    pub amm_config: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct OpenPool<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut, seeds = [MINE_SEED, mint.key().as_ref()], bump = mine.bump)]
    pub mine: Box<Account<'info, Mine>>,
    /// CHECK: the mine's vault PDA
    #[account(mut, seeds = [VAULT_SEED, mine.key().as_ref()], bump = mine.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: program mint authority
    #[account(seeds = [AUTH_SEED], bump)]
    pub auth: UncheckedAccount<'info>,
    #[account(mut, address = mine.mint)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(address = WSOL)]
    pub wsol_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        init_if_needed,
        payer = creator,
        associated_token::mint = wsol_mint,
        associated_token::authority = vault,
        associated_token::token_program = classic_token_program,
    )]
    pub vault_wsol: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = creator,
        associated_token::mint = mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_token: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: the vault's LP token account, created by Raydium.
    #[account(mut)]
    pub vault_lp: UncheckedAccount<'info>,
    /// CHECK: Raydium accounts, verified by Raydium and by address below.
    pub amm_config: UncheckedAccount<'info>,
    /// CHECK: Raydium pool authority
    pub ray_authority: UncheckedAccount<'info>,
    /// CHECK: Raydium pool state PDA, created by Raydium
    #[account(mut)]
    pub pool_state: UncheckedAccount<'info>,
    /// CHECK: created by Raydium
    #[account(mut)]
    pub lp_mint: UncheckedAccount<'info>,
    /// CHECK: created by Raydium
    #[account(mut)]
    pub pool_vault_0: UncheckedAccount<'info>,
    /// CHECK: created by Raydium
    #[account(mut)]
    pub pool_vault_1: UncheckedAccount<'info>,
    /// CHECK: Raydium's fee receiver, checked by Raydium
    #[account(mut)]
    pub create_pool_fee: UncheckedAccount<'info>,
    /// CHECK: created by Raydium
    #[account(mut)]
    pub observation: UncheckedAccount<'info>,
    /// CHECK: Raydium CPMM
    #[account(address = RAYDIUM_CPMM)]
    pub raydium: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token2022>,
    pub classic_token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct SetMedia<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(seeds = [MINE_SEED, mine.mint.as_ref()], bump = mine.bump, has_one = creator @ SolzError::NotCreator)]
    pub mine: Box<Account<'info, Mine>>,
    /// CHECK: the token's mint, which holds its metadata
    #[account(mut, address = mine.mint)]
    pub mint: UncheckedAccount<'info>,
    /// CHECK: program metadata authority
    #[account(seeds = [AUTH_SEED], bump)]
    pub auth: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

// ------------------------------------------------------------------ mining

#[derive(Accounts)]
pub struct MineSol<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, seeds = [MINE_SEED, mine.mint.as_ref()], bump = mine.bump)]
    pub mine: Box<Account<'info, Mine>>,
    /// CHECK: the mine's vault PDA
    #[account(mut, seeds = [VAULT_SEED, mine.key().as_ref()], bump = mine.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = user,
        space = 8 + Miner::INIT_SPACE,
        seeds = [b"miner", mine.key().as_ref(), user.key().as_ref()],
        bump
    )]
    pub miner: Box<Account<'info, Miner>>,
    #[account(
        init,
        payer = user,
        space = 8 + Position::INIT_SPACE,
        seeds = [b"pos", mine.key().as_ref(), user.key().as_ref(), &miner.next_id.to_le_bytes()],
        bump
    )]
    pub position: Box<Account<'info, Position>>,
    #[account(seeds = [b"global"], bump = global.bump)]
    pub global: Box<Account<'info, Global>>,
    #[account(mut)]
    pub solz_mine: Option<Box<Account<'info, Mine>>>,
    /// CHECK: checked against solz_mine in the handler
    #[account(mut)]
    pub solz_vault: Option<UncheckedAccount<'info>>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, seeds = [MINE_SEED, mine.mint.as_ref()], bump = mine.bump)]
    pub mine: Box<Account<'info, Mine>>,
    /// CHECK: the mine's vault PDA
    #[account(mut, seeds = [VAULT_SEED, mine.key().as_ref()], bump = mine.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    #[account(mut, seeds = [b"miner", mine.key().as_ref(), user.key().as_ref()], bump = miner.bump)]
    pub miner: Box<Account<'info, Miner>>,
    /// CHECK: program mint authority
    #[account(seeds = [AUTH_SEED], bump)]
    pub auth: UncheckedAccount<'info>,
    #[account(mut, address = mine.mint)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint,
        associated_token::authority = user,
        associated_token::token_program = token_program,
    )]
    pub user_token: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Program<'info, Token2022>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetAuto<'info> {
    pub user: Signer<'info>,
    #[account(seeds = [MINE_SEED, mine.mint.as_ref()], bump = mine.bump)]
    pub mine: Box<Account<'info, Mine>>,
    #[account(mut, has_one = mine @ SolzError::WrongAccount, constraint = position.owner == user.key() @ SolzError::NotOwner)]
    pub position: Box<Account<'info, Position>>,
}

#[derive(Accounts)]
pub struct Roll<'info> {
    pub caller: Signer<'info>,
    #[account(mut, seeds = [MINE_SEED, mine.mint.as_ref()], bump = mine.bump)]
    pub mine: Box<Account<'info, Mine>>,
    /// CHECK: the mine's vault PDA
    #[account(mut, seeds = [VAULT_SEED, mine.key().as_ref()], bump = mine.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    #[account(mut, seeds = [b"miner", mine.key().as_ref(), position.owner.as_ref()], bump = miner.bump)]
    pub miner: Box<Account<'info, Miner>>,
    #[account(mut, has_one = mine @ SolzError::WrongAccount)]
    pub position: Box<Account<'info, Position>>,
    #[account(seeds = [b"global"], bump = global.bump)]
    pub global: Box<Account<'info, Global>>,
    #[account(mut)]
    pub solz_mine: Option<Box<Account<'info, Mine>>>,
    /// CHECK: checked against solz_mine in the handler
    #[account(mut)]
    pub solz_vault: Option<UncheckedAccount<'info>>,
    pub system_program: Program<'info, System>,
}

// ------------------------------------------------------------------ staking

#[derive(Accounts)]
pub struct StakeIx<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, seeds = [MINE_SEED, mine.mint.as_ref()], bump = mine.bump)]
    pub mine: Box<Account<'info, Mine>>,
    /// CHECK: the mine's vault PDA, owner of the stake vault
    #[account(seeds = [VAULT_SEED, mine.key().as_ref()], bump = mine.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = user,
        space = 8 + Stake::INIT_SPACE,
        seeds = [b"stake", mine.key().as_ref(), user.key().as_ref()],
        bump
    )]
    pub stake: Box<Account<'info, Stake>>,
    #[account(address = mine.mint)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        init_if_needed,
        payer = user,
        seeds = [b"stake_vault", mine.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = vault,
        token::token_program = token_program,
    )]
    pub stake_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint,
        associated_token::authority = user,
        associated_token::token_program = token_program,
    )]
    pub user_token: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Program<'info, Token2022>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimDividends<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, seeds = [MINE_SEED, mine.mint.as_ref()], bump = mine.bump)]
    pub mine: Box<Account<'info, Mine>>,
    /// CHECK: the mine's vault PDA
    #[account(mut, seeds = [VAULT_SEED, mine.key().as_ref()], bump = mine.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    #[account(mut, seeds = [b"stake", mine.key().as_ref(), user.key().as_ref()], bump = stake.bump)]
    pub stake: Box<Account<'info, Stake>>,
    pub system_program: Program<'info, System>,
}

// ------------------------------------------------------------------ keepers

#[derive(Accounts)]
pub struct Keeper<'info> {
    pub caller: Signer<'info>,
    #[account(mut, seeds = [MINE_SEED, mine.mint.as_ref()], bump = mine.bump)]
    pub mine: Box<Account<'info, Mine>>,
    /// CHECK: the mine's vault PDA
    #[account(mut, seeds = [VAULT_SEED, mine.key().as_ref()], bump = mine.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    #[account(mut, address = mine.mint)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(address = WSOL)]
    pub wsol_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, associated_token::mint = wsol_mint, associated_token::authority = vault, associated_token::token_program = classic_token_program)]
    pub vault_wsol: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, associated_token::mint = mint, associated_token::authority = vault, associated_token::token_program = token_program)]
    pub vault_token: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: the vault's LP account (deepen only), checked against the pool's LP mint.
    #[account(mut)]
    pub vault_lp: UncheckedAccount<'info>,
    /// CHECK: Raydium
    pub amm_config: UncheckedAccount<'info>,
    /// CHECK: Raydium
    pub ray_authority: UncheckedAccount<'info>,
    /// CHECK: Raydium, must be the mine's pool
    #[account(mut, address = mine.pool)]
    pub pool_state: UncheckedAccount<'info>,
    /// CHECK: must be the pool's token vault
    #[account(mut, address = mine.pool_token_vault)]
    pub pool_token_vault: UncheckedAccount<'info>,
    /// CHECK: must be the pool's SOL vault
    #[account(mut, address = mine.pool_sol_vault)]
    pub pool_sol_vault: UncheckedAccount<'info>,
    /// CHECK: must be the pool's LP mint
    #[account(mut, address = mine.lp_mint)]
    pub lp_mint: UncheckedAccount<'info>,
    /// CHECK: must be the pool's observation account
    #[account(mut, address = mine.observation)]
    pub observation: UncheckedAccount<'info>,
    /// CHECK: Raydium CPMM
    #[account(address = RAYDIUM_CPMM)]
    pub raydium: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token2022>,
    pub classic_token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
