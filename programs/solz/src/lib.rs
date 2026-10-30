//! SOLZ: mined with SOL, never minted.
//!
//! Commit SOL for 24 hours and take 90 percent back with your tokens. 32 cycles
//! for $SOLZ, each with half the allocation of the one before at twice the
//! rate. A cycle ends only when it is mined out. The 10 percent kept at commit
//! time is split at once: liquidity (paired with SOL on Raydium, LP burned),
//! buy and burn, stakers, operations, and for launched tokens the token's
//! creator and the $SOLZ burn.
//!
//! The same engine runs every token on the launchpad. There is no owner, no
//! pause and no upgrade: the program is deployed with its upgrade authority
//! removed, and every rule is a constant.
use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod math;
pub mod ray;
pub mod state;
pub mod ix;

pub use ix::*;

declare_id!("33jmYNoZWJJB74EaQkcVNXBVka6sst5gzWhqZBAUPtTp");

#[program]
pub mod solz {
    use super::*;

    pub fn init_global(ctx: Context<InitGlobal>) -> Result<()> {
        ix::admin::init_global(ctx)
    }

    /// Step one of a launch: the token, its metadata, its mine and its vault.
    /// `kind` 0 is $SOLZ itself (launcher only, once); `kind` 1 is a launchpad token.
    pub fn create_mine<'info>(
        ctx: Context<'info, CreateMine<'info>>,
        kind: u8,
        name: String,
        symbol: String,
        uri: String,
        presets: [u8; 3],
        media: Vec<String>,
    ) -> Result<()> {
        ix::launch::create_mine(ctx, kind, name, symbol, uri, presets, media)
    }

    /// Step two: seed the Raydium pool, burn the LP, open mining. The creator's
    /// own first mine goes in the same transaction as a separate instruction.
    pub fn open_pool<'info>(ctx: Context<'info, OpenPool<'info>>, seed_sol: u64, fund: u64) -> Result<()> {
        ix::launch::open_pool(ctx, seed_sol, fund)
    }

    pub fn mine<'info>(ctx: Context<'info, MineSol<'info>>, amount: u64, auto_through: u8) -> Result<()> {
        ix::mining::mine(ctx, amount, auto_through)
    }

    /// Collect every matured position passed in remaining accounts, plus any
    /// tokens and refunds waiting from auto rolls.
    pub fn claim<'info>(ctx: Context<'info, Claim<'info>>) -> Result<()> {
        ix::mining::claim(ctx)
    }

    pub fn set_auto(ctx: Context<SetAuto>, auto_through: u8) -> Result<()> {
        ix::mining::set_auto(ctx, auto_through)
    }

    /// Anyone may roll a matured auto position: its tokens to its owner's
    /// account, its 90 percent back into the current cycle in the owner's name.
    pub fn roll<'info>(ctx: Context<'info, Roll<'info>>) -> Result<()> {
        ix::mining::roll(ctx)
    }

    pub fn stake<'info>(ctx: Context<'info, StakeIx<'info>>, amount: u64) -> Result<()> {
        ix::staking::stake(ctx, amount)
    }

    pub fn unstake<'info>(ctx: Context<'info, StakeIx<'info>>, amount: u64) -> Result<()> {
        ix::staking::unstake(ctx, amount)
    }

    pub fn claim_dividends(ctx: Context<ClaimDividends>) -> Result<()> {
        ix::staking::claim_dividends(ctx)
    }

    /// Buy and burn: runs only while the pool price is under the true cost.
    pub fn buy_burn<'info>(ctx: Context<'info, Keeper<'info>>) -> Result<()> {
        ix::keepers::buy_burn(ctx, false)
    }

    /// The launchpad burner for $SOLZ: buys at any price, in small steps.
    pub fn pad_burn<'info>(ctx: Context<'info, Keeper<'info>>) -> Result<()> {
        ix::keepers::buy_burn(ctx, true)
    }

    /// Buy with half the queue, pair it with SOL, burn the LP.
    pub fn deepen<'info>(ctx: Context<'info, Keeper<'info>>) -> Result<()> {
        ix::keepers::deepen(ctx)
    }

    /// Only if Raydium switches the pool off: frees the queues that need it.
    pub fn release_stuck<'info>(ctx: Context<'info, Keeper<'info>>) -> Result<()> {
        ix::keepers::release_stuck(ctx)
    }

    /// Anyone may push what operations is owed to the operations wallet.
    pub fn withdraw_ops(ctx: Context<WithdrawTo>) -> Result<()> {
        ix::admin::withdraw(ctx, 0)
    }

    /// Anyone may push what the creator is owed to the creator.
    pub fn withdraw_creator(ctx: Context<WithdrawTo>) -> Result<()> {
        ix::admin::withdraw(ctx, 1)
    }

    /// The creator may change the token's image, banner, description and links.
    /// Name, ticker and every rule are fixed.
    pub fn set_media(ctx: Context<SetMedia>, field: String, value: String) -> Result<()> {
        ix::launch::set_media(ctx, field, value)
    }
}
