use crate::constants::*;
use crate::errors::SolzError;
use crate::events::*;
use crate::math::bps_of;
use crate::state::Mine;
use anchor_lang::prelude::*;
use anchor_lang::system_program;

pub const VAULT_SEED: &[u8] = b"vault";
pub const MINE_SEED: &[u8] = b"mine";
pub const AUTH_SEED: &[u8] = b"auth";

/// Lamports from a wallet that signed this transaction.
pub fn pay_in<'info>(from: &AccountInfo<'info>, to: &AccountInfo<'info>, sys: &AccountInfo<'info>, amount: u64) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    system_program::transfer(
        CpiContext::new(sys.key(), system_program::Transfer { from: from.clone(), to: to.clone() }),
        amount,
    )
}

/// Lamports out of a mine's vault (a system owned PDA with no data).
pub fn pay_out<'info>(
    vault: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    sys: &AccountInfo<'info>,
    mine_key: &Pubkey,
    vault_bump: u8,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let seeds: &[&[u8]] = &[VAULT_SEED, mine_key.as_ref(), &[vault_bump]];
    system_program::transfer(
        CpiContext::new_with_signer(sys.key(), system_program::Transfer { from: vault.clone(), to: to.clone() }, &[seeds]),
        amount,
    )
}

/// What the vault must hold, in lamports, beyond its rent: the whole ledger.
pub fn owed(m: &Mine) -> u64 {
    m.principal_owed + m.ops_owed + m.creator_owed + m.refunds_owed + m.burn_queue + m.lp_queue + m.pad_burn_queue
        + m.div_pending + m.div_unpaid
}

/// Where SOL comes from for the retained 10 percent.
pub enum Payer<'a, 'info> {
    /// A wallet paying in this transaction: it pays the vault, and the $SOLZ share directly.
    Wallet(&'a AccountInfo<'info>),
    /// Already in the vault (a roll).
    Vault,
}

/// Split a commit of `amount` whose `principal` is already booked. Every line
/// is credited on the ledger; the stake line takes the remainder so the lines
/// always add up to exactly what was committed.
#[allow(clippy::too_many_arguments)]
pub fn route<'a, 'info>(
    m: &mut Mine,
    mine_key: &Pubkey,
    amount: u64,
    principal: u64,
    vault: &AccountInfo<'info>,
    sys: &AccountInfo<'info>,
    solz: Option<(&mut Mine, &AccountInfo<'info>)>,
    source: Payer<'a, 'info>,
) -> Result<()> {
    let b = m.bps();
    let lp = bps_of(amount, b.lp);
    let burn = bps_of(amount, b.burn);
    let to_solz = bps_of(amount, b.solz);
    let ops = bps_of(amount, b.ops);
    let creator = bps_of(amount, b.creator);
    let stake = amount - principal - lp - burn - to_solz - ops - creator;

    m.lp_queue += lp;
    m.burn_queue += burn;
    m.ops_owed += ops;
    m.ops_total += ops;
    m.creator_owed += creator;
    m.creator_total += creator;
    m.div_total += stake;
    distribute(m, stake);

    // Everything but the $SOLZ share stays in (or goes to) this vault.
    let into_vault = amount - to_solz;
    if let Payer::Wallet(w) = &source {
        pay_in(w, vault, sys, into_vault)?;
    }
    if to_solz > 0 {
        let (sm, sv) = solz.ok_or(SolzError::WrongAccount)?;
        match source {
            Payer::Wallet(w) => pay_in(w, sv, sys, to_solz)?,
            Payer::Vault => pay_out(vault, sv, sys, mine_key, m.vault_bump, to_solz)?,
        }
        sm.pad_burn_queue += to_solz;
        m.sent_to_solz += to_solz;
    }
    Ok(())
}

/// Stakers' share. With nobody staked it goes to the buy and burn instead, so no
/// pile builds up for a first staker to take.
pub fn distribute(m: &mut Mine, amount: u64) {
    let div = amount + m.div_pending;
    m.div_pending = 0;
    if div == 0 {
        return;
    }
    if (m.total_staked as u128) >= MIN_STAKED {
        m.acc_per_share += (div as u128) * MAG / (m.total_staked as u128);
        m.div_unpaid += div;
    } else {
        m.burn_queue += div;
    }
}

pub fn check_solvent(m: &Mine, vault: &AccountInfo, wsol_float: u64) -> Result<()> {
    let rent = Rent::get()?.minimum_balance(0);
    require!(vault.lamports() + wsol_float >= owed(m) + rent, SolzError::Insolvent);
    Ok(())
}

pub fn emit_mined(m: &Mine, owner: Pubkey, position: Pubkey, amount: u64, reward: u64, auto_through: u8, rolled: bool) {
    emit!(Mined {
        mine: m.mint,
        owner,
        position,
        amount,
        reward,
        cycle_after: m.cycle,
        auto_through,
        rolled,
    });
}
