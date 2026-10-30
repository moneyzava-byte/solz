use super::util::*;
use super::*;
use crate::events::*;

/// $SOLZ's mine and vault, needed only by launched tokens (their 1 percent).
fn solz_pair<'a, 'info>(
    kind: u8,
    global: &Global,
    solz_mine: &'a mut Option<Box<Account<'info, Mine>>>,
    solz_vault: &'a Option<UncheckedAccount<'info>>,
) -> Result<Option<(&'a mut Mine, &'a AccountInfo<'info>)>> {
    if kind == 0 {
        return Ok(None);
    }
    let sm = solz_mine.as_mut().ok_or(SolzError::WrongAccount)?;
    let sv = solz_vault.as_ref().ok_or(SolzError::WrongAccount)?;
    require_keys_eq!(sm.key(), global.solz_mine, SolzError::WrongAccount);
    let (expect, _) = Pubkey::find_program_address(&[VAULT_SEED, sm.key().as_ref()], &crate::ID);
    require_keys_eq!(sv.key(), expect, SolzError::WrongAccount);
    Ok(Some((&mut ***sm, sv.as_ref())))
}

pub fn mine<'info>(ctx: Context<'info, MineSol<'info>>, amount: u64, auto_through: u8) -> Result<()> {
    let a = &mut *ctx.accounts;
    require!(a.mine.open, SolzError::NotOpen);
    require!(amount >= MIN_MINE, SolzError::TooSmall);
    require!(auto_through <= a.mine.cycles, SolzError::BadAutoThrough);
    if auto_through != 0 {
        require!(amount >= MIN_AUTO, SolzError::BelowAutoMinimum);
    }
    let now = Clock::get()?.unix_timestamp;
    let mine_key = a.mine.key();

    // Price first: anything past the final cycle is simply never taken.
    let (reward, used) = a.mine.price(amount)?;
    let principal = ((used as u128) * BPS_RETURN / BPS) as u64;

    let miner = &mut a.miner;
    if miner.owner == Pubkey::default() {
        miner.mine = mine_key;
        miner.owner = a.user.key();
        miner.bump = ctx.bumps.miner;
    }
    let id = miner.next_id;
    miner.next_id += 1;
    miner.open_positions += 1;

    let p = &mut a.position;
    p.mine = mine_key;
    p.owner = a.user.key();
    p.id = id;
    p.bump = ctx.bumps.position;
    p.auto_through = auto_through;
    p.cycle = a.mine.cycle.min(a.mine.cycles);
    p.principal = principal;
    p.reward = reward;
    p.committed = used;
    p.maturity = now + LOCK;

    let m = &mut a.mine;
    m.principal_owed += principal;
    m.rewards_owed += reward;
    m.mines_count += 1;
    m.committed += used;
    let kind = m.kind;
    let solz = solz_pair(kind, &a.global, &mut a.solz_mine, &a.solz_vault)?;
    let user = a.user.to_account_info();
    route(
        m,
        &mine_key,
        used,
        principal,
        &a.vault.to_account_info(),
        &a.system_program.to_account_info(),
        solz,
        Payer::Wallet(&user),
    )?;
    emit_mined(m, a.user.key(), a.position.key(), used, reward, auto_through, false);
    Ok(())
}

pub fn claim<'info>(ctx: Context<'info, Claim<'info>>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let a = &mut *ctx.accounts;
    let mine_key = a.mine.key();
    let user_key = a.user.key();
    let mut principal: u64 = 0;
    let mut reward: u64 = 0;
    let mut closed: u32 = 0;

    let mut seen: Vec<Pubkey> = Vec::with_capacity(ctx.remaining_accounts.len());
    for info in ctx.remaining_accounts.iter() {
        require!(info.is_writable, SolzError::WrongAccount);
        require!(!seen.contains(info.key), SolzError::WrongAccount);
        seen.push(*info.key);
        let pos: Account<Position> = Account::try_from(info)?;
        require_keys_eq!(pos.mine, mine_key, SolzError::WrongAccount);
        require_keys_eq!(pos.owner, user_key, SolzError::NotOwner);
        require!(now >= pos.maturity, SolzError::NotMatured);
        principal += pos.principal;
        reward += pos.reward;
        closed += 1;
    }

    let miner = &mut a.miner;
    let rolled_reward = miner.reward_owed;
    let refund = miner.refund_owed;
    miner.reward_owed = 0;
    miner.refund_owed = 0;
    miner.open_positions = miner.open_positions.saturating_sub(closed);
    reward += rolled_reward;
    require!(principal > 0 || reward > 0 || refund > 0, SolzError::NothingMatured);

    let m = &mut a.mine;
    m.principal_owed -= principal;
    m.rewards_owed -= reward;
    m.refunds_owed -= refund;

    if reward > 0 {
        let auth_seeds: &[&[u8]] = &[AUTH_SEED, &[ctx.bumps.auth]];
        anchor_spl::token_2022::mint_to(
            CpiContext::new_with_signer(
                a.token_program.key(),
                anchor_spl::token_2022::MintTo {
                    mint: a.mint.to_account_info(),
                    to: a.user_token.to_account_info(),
                    authority: a.auth.to_account_info(),
                },
                &[auth_seeds],
            ),
            reward,
        )?;
    }
    pay_out(
        &a.vault.to_account_info(),
        &a.user.to_account_info(),
        &a.system_program.to_account_info(),
        &mine_key,
        m.vault_bump,
        principal + refund,
    )?;
    // Close the positions last: their rent goes back to the miner.
    for info in ctx.remaining_accounts.iter() {
        let pos: Account<Position> = Account::try_from(info)?;
        pos.close(a.user.to_account_info())?;
    }
    emit!(Claimed { mine: m.mint, owner: user_key, principal: principal + refund, reward });
    Ok(())
}

pub fn set_auto(ctx: Context<SetAuto>, auto_through: u8) -> Result<()> {
    let m = &ctx.accounts.mine;
    let p = &mut ctx.accounts.position;
    require!(auto_through <= m.cycles, SolzError::BadAutoThrough);
    if auto_through != 0 {
        require!(p.principal >= MIN_AUTO, SolzError::BelowAutoMinimum);
    }
    p.auto_through = auto_through;
    emit!(AutoSet { mine: m.mint, position: p.key(), auto_through });
    Ok(())
}

pub fn roll<'info>(ctx: Context<'info, Roll<'info>>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let a = &mut *ctx.accounts;
    let mine_key = a.mine.key();
    {
        let p = &a.position;
        require!(now >= p.maturity, SolzError::NotMatured);
        require!(
            p.auto_through != 0 && p.auto_through >= a.mine.cycle && a.mine.cycle <= a.mine.cycles,
            SolzError::AutoEnded
        );
        require_keys_eq!(a.miner.owner, p.owner, SolzError::WrongAccount);
    }
    let old_principal = a.position.principal;
    let old_reward = a.position.reward;

    // The matured tokens wait on the miner's account until its next claim.
    a.miner.reward_owed += old_reward;

    let m = &mut a.mine;
    m.principal_owed -= old_principal;
    let (reward, used) = m.price(old_principal)?;
    let unused = old_principal - used;
    if unused > 0 {
        a.miner.refund_owed += unused;
        m.refunds_owed += unused;
    }
    let principal = ((used as u128) * BPS_RETURN / BPS) as u64;
    m.principal_owed += principal;
    m.rewards_owed += reward;
    m.mines_count += 1;
    m.committed += used;

    let p = &mut a.position;
    // Auto sunsets by itself once the rolled principal is below the minimum.
    if principal < MIN_AUTO {
        p.auto_through = 0;
    }
    p.principal = principal;
    p.reward = reward;
    p.committed = used;
    p.cycle = m.cycle.min(m.cycles);
    p.maturity = now + LOCK;
    p.rolls += 1;
    let auto_through = p.auto_through;
    let owner = p.owner;

    let kind = m.kind;
    let solz = solz_pair(kind, &a.global, &mut a.solz_mine, &a.solz_vault)?;
    route(
        m,
        &mine_key,
        used,
        principal,
        &a.vault.to_account_info(),
        &a.system_program.to_account_info(),
        solz,
        Payer::Vault,
    )?;
    emit_mined(m, owner, a.position.key(), used, reward, auto_through, true);
    Ok(())
}
