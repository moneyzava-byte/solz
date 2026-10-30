use anchor_lang::prelude::*;

/// One per token mined here: $SOLZ itself (kind 0) or a launchpad token (kind 1).
/// Every lamport the mine's vault holds above rent belongs to exactly one line below.
#[account]
#[derive(InitSpace)]
pub struct Mine {
    pub kind: u8,
    pub bump: u8,
    pub vault_bump: u8,
    pub open: bool,
    pub token_is_0: bool,
    pub cycle: u8,
    pub cycles: u8,
    pub _pad: u8,

    pub mint: Pubkey,
    pub creator: Pubkey,
    pub pool: Pubkey,
    pub pool_token_vault: Pubkey,
    pub pool_sol_vault: Pubkey,
    pub lp_mint: Pubkey,
    pub observation: Pubkey,

    pub cap: u64,
    pub max_seed: u64,
    /// Nanolamports per whole token in cycle 1.
    pub rate0: u128,

    /// Tokens priced and promised across all cycles so far (seed included).
    pub total_reserved: u64,
    /// Tokens promised to open positions and not minted yet.
    pub rewards_owed: u64,

    // ---- the ledger, in lamports
    pub principal_owed: u64,
    pub ops_owed: u64,
    pub creator_owed: u64,
    pub refunds_owed: u64,
    pub burn_queue: u64,
    pub lp_queue: u64,
    /// $SOLZ only: launch fees and 1 percent of every launched token's mines.
    pub pad_burn_queue: u64,
    /// Staker dividends that waited for the first staker.
    pub div_pending: u64,
    /// Distributed to stakers and not yet paid out.
    pub div_unpaid: u64,

    // ---- staking
    pub acc_per_share: u128,
    pub total_staked: u64,

    // ---- keepers
    pub last_burn: i64,
    pub last_deepen: i64,
    pub last_pad_burn: i64,

    // ---- totals, for the site
    pub launched_at: i64,
    pub mines_count: u64,
    pub committed: u64,
    pub total_burned: u64,
    pub burn_spent: u64,
    pub pad_burned: u64,
    pub pad_spent: u64,
    pub lp_sol_added: u64,
    pub lp_tokens_added: u64,
    pub div_total: u64,
    pub ops_total: u64,
    pub creator_total: u64,
    pub sent_to_solz: u64,
}

/// One per wallet per mine.
#[account]
#[derive(InitSpace)]
pub struct Miner {
    pub mine: Pubkey,
    pub owner: Pubkey,
    pub bump: u8,
    pub next_id: u64,
    pub open_positions: u32,
    /// Tokens from auto rolls, waiting to be minted to the wallet on the next claim.
    pub reward_owed: u64,
    /// SOL that a roll could not commit because mining had ended.
    pub refund_owed: u64,
}

#[account]
#[derive(InitSpace)]
pub struct Position {
    pub mine: Pubkey,
    pub owner: Pubkey,
    pub id: u64,
    pub bump: u8,
    /// 0 = manual; N = roll automatically while cycle <= N.
    pub auto_through: u8,
    pub cycle: u8,
    pub principal: u64,
    pub reward: u64,
    pub committed: u64,
    pub maturity: i64,
    pub rolls: u32,
}

#[account]
#[derive(InitSpace)]
pub struct Stake {
    pub mine: Pubkey,
    pub owner: Pubkey,
    pub bump: u8,
    pub amount: u64,
    pub debt: u128,
    pub owed: u64,
    pub paid: u64,
    pub last_stake: i64,
}

/// Reserves a ticker for one token, forever, case blind.
#[account]
#[derive(InitSpace)]
pub struct Symbol {
    pub mint: Pubkey,
    pub bump: u8,
}

/// Program wide counters.
#[account]
#[derive(InitSpace)]
pub struct Global {
    pub bump: u8,
    pub solz_mine: Pubkey,
    pub launches: u64,
}
