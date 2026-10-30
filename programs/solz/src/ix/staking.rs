use super::util::*;
use super::*;
use crate::events::*;

/// amount * acc / 2^64 without overflowing u128 on the way.
pub fn accrued(amount: u64, acc: u128) -> u128 {
    let a = amount as u128;
    let hi = acc >> 64;
    let lo = acc & (u64::MAX as u128);
    a * hi + ((a * lo) >> 64)
}

/// Bring a staker's earnings up to date with the accumulator.
fn settle(m: &Mine, s: &mut Stake) {
    let accrued = accrued(s.amount, m.acc_per_share);
    if accrued > s.debt {
        s.owed += (accrued - s.debt) as u64;
    }
    s.debt = accrued;
}

pub fn stake<'info>(ctx: Context<'info, StakeIx<'info>>, amount: u64) -> Result<()> {
    require!(amount > 0, SolzError::TooSmall);
    let a = &mut *ctx.accounts;
    let s = &mut a.stake;
    if s.owner == Pubkey::default() {
        s.mine = a.mine.key();
        s.owner = a.user.key();
        s.bump = ctx.bumps.stake;
    }
    settle(&a.mine, s);
    anchor_spl::token_2022::transfer_checked(
        CpiContext::new(
            a.token_program.key(),
            anchor_spl::token_2022::TransferChecked {
                from: a.user_token.to_account_info(),
                mint: a.mint.to_account_info(),
                to: a.stake_vault.to_account_info(),
                authority: a.user.to_account_info(),
            },
        ),
        amount,
        DECIMALS,
    )?;
    s.amount += amount;
    s.debt = accrued(s.amount, a.mine.acc_per_share);
    // A stake earns from the next mine on, and can leave 24 hours after it was last added to.
    s.last_stake = Clock::get()?.unix_timestamp;
    let m = &mut a.mine;
    m.total_staked += amount;
    emit!(Staked { mine: m.mint, owner: a.user.key(), amount, total: s.amount });
    Ok(())
}

pub fn unstake<'info>(ctx: Context<'info, StakeIx<'info>>, amount: u64) -> Result<()> {
    require!(amount > 0, SolzError::TooSmall);
    let a = &mut *ctx.accounts;
    let s = &mut a.stake;
    require!(s.amount >= amount, SolzError::NotEnoughStaked);
    require!(Clock::get()?.unix_timestamp >= s.last_stake + LOCK, SolzError::StakeLocked);
    settle(&a.mine, s);
    let mine_key = a.mine.key();
    let vault_seeds: &[&[u8]] = &[VAULT_SEED, mine_key.as_ref(), &[a.mine.vault_bump]];
    anchor_spl::token_2022::transfer_checked(
        CpiContext::new_with_signer(
            a.token_program.key(),
            anchor_spl::token_2022::TransferChecked {
                from: a.stake_vault.to_account_info(),
                mint: a.mint.to_account_info(),
                to: a.user_token.to_account_info(),
                authority: a.vault.to_account_info(),
            },
            &[vault_seeds],
        ),
        amount,
        DECIMALS,
    )?;
    s.amount -= amount;
    s.debt = accrued(s.amount, a.mine.acc_per_share);
    a.mine.total_staked -= amount;
    emit!(Unstaked { mine: a.mine.mint, owner: a.user.key(), amount, total: s.amount });
    Ok(())
}

pub fn claim_dividends(ctx: Context<ClaimDividends>) -> Result<()> {
    let a = &mut *ctx.accounts;
    settle(&a.mine, &mut a.stake);
    let amount = a.stake.owed;
    require!(amount > 0, SolzError::NothingToWithdraw);
    a.stake.owed = 0;
    a.stake.paid += amount;
    let m = &mut a.mine;
    // Rounding in the accumulator can only ever leave dust behind, never pay more
    // than was distributed; the clamp makes that explicit.
    let pay = amount.min(m.div_unpaid);
    m.div_unpaid -= pay;
    pay_out(
        &a.vault.to_account_info(),
        &a.user.to_account_info(),
        &a.system_program.to_account_info(),
        &m.key(),
        m.vault_bump,
        pay,
    )?;
    emit!(DividendsPaid { mine: m.mint, owner: a.user.key(), amount: pay });
    Ok(())
}
