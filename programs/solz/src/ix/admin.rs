use super::util::*;
use super::*;
use crate::events::*;

pub fn init_global(ctx: Context<InitGlobal>) -> Result<()> {
    let g = &mut ctx.accounts.global;
    g.bump = ctx.bumps.global;
    g.solz_mine = Pubkey::default();
    g.launches = 0;
    Ok(())
}

/// 0 = operations share to the operations wallet, 1 = creator share to the creator.
pub fn withdraw(ctx: Context<WithdrawTo>, what: u8) -> Result<()> {
    let a = &mut *ctx.accounts;
    let m = &mut a.mine;
    let (expect, amount) = if what == 0 { (OPS, m.ops_owed) } else { (m.creator, m.creator_owed) };
    require_keys_eq!(a.to.key(), expect, SolzError::WrongAccount);
    require!(amount > 0, SolzError::NothingToWithdraw);
    if what == 0 {
        m.ops_owed = 0;
    } else {
        m.creator_owed = 0;
    }
    pay_out(&a.vault.to_account_info(), &a.to.to_account_info(), &a.system_program.to_account_info(), &m.key(), m.vault_bump, amount)?;
    emit!(Withdrawn { mine: m.mint, to: expect, amount, what });
    Ok(())
}
