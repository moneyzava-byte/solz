use super::util::*;
use super::*;
use crate::events::*;
use crate::math::amount_out;
use crate::ray;

struct Ctx<'a, 'info> {
    a: &'a Keeper<'info>,
    mine_key: Pubkey,
    vault_bump: u8,
}

impl<'a, 'info> Ctx<'a, 'info> {
    fn vault_seeds(&self) -> [&[u8]; 3] {
        // `vault_bump` lives as long as self, so the slice can borrow from it.
        [VAULT_SEED, self.mine_key.as_ref(), std::slice::from_ref(&self.vault_bump)]
    }

    fn view(&self) -> Result<ray::PoolView> {
        ray::view(
            &self.a.pool_state.to_account_info(),
            &self.a.pool_token_vault.to_account_info(),
            &self.a.pool_sol_vault.to_account_info(),
            self.a.mine.token_is_0,
        )
    }

    fn wsol(&self) -> Result<u64> {
        ray::token_amount(&self.a.vault_wsol.to_account_info())
    }

    fn tokens(&self) -> Result<u64> {
        ray::token_amount(&self.a.vault_token.to_account_info())
    }

    /// Make sure the vault's wrapped SOL account holds at least `need`.
    /// Returns the lamports moved in.
    fn fund_wsol(&self, need: u64) -> Result<u64> {
        let have = self.wsol()?;
        if have >= need {
            return Ok(0);
        }
        let add = need - have;
        pay_out(
            &self.a.vault.to_account_info(),
            &self.a.vault_wsol.to_account_info(),
            &self.a.system_program.to_account_info(),
            &self.mine_key,
            self.vault_bump,
            add,
        )?;
        anchor_spl::token::sync_native(CpiContext::new(
            self.a.classic_token_program.key(),
            anchor_spl::token::SyncNative { account: self.a.vault_wsol.to_account_info() },
        ))?;
        Ok(add)
    }

    /// Spend exactly `sol_in` wrapped SOL on tokens, delivered to the vault's token account.
    fn buy(&self, sol_in: u64, min_out: u64) -> Result<u64> {
        let before = self.tokens()?;
        let seeds = self.vault_seeds();
        let a = self.a;
        ray::swap_base_input(
            ray::SwapAccounts {
                payer: &a.vault.to_account_info(),
                authority: &a.ray_authority.to_account_info(),
                amm_config: &a.amm_config.to_account_info(),
                pool_state: &a.pool_state.to_account_info(),
                input_token_account: &a.vault_wsol.to_account_info(),
                output_token_account: &a.vault_token.to_account_info(),
                input_vault: &a.pool_sol_vault.to_account_info(),
                output_vault: &a.pool_token_vault.to_account_info(),
                input_token_program: &a.classic_token_program.to_account_info(),
                output_token_program: &a.token_program.to_account_info(),
                input_token_mint: &a.wsol_mint.to_account_info(),
                output_token_mint: &a.mint.to_account_info(),
                observation_state: &a.observation.to_account_info(),
                raydium: &a.raydium.to_account_info(),
            },
            sol_in,
            min_out,
            &[&seeds],
        )?;
        let out = self.tokens()?.checked_sub(before).ok_or(SolzError::ShortOutput)?;
        require!(out >= min_out && out > 0, SolzError::ShortOutput);
        Ok(out)
    }

    /// Burn every token the vault's token account holds (it never holds any between calls).
    fn burn_tokens(&self) -> Result<u64> {
        let amount = self.tokens()?;
        if amount == 0 {
            return Ok(0);
        }
        let seeds = self.vault_seeds();
        anchor_spl::token_2022::burn(
            CpiContext::new_with_signer(
                self.a.token_program.key(),
                anchor_spl::token_2022::Burn {
                    mint: self.a.mint.to_account_info(),
                    from: self.a.vault_token.to_account_info(),
                    authority: self.a.vault.to_account_info(),
                },
                &[&seeds],
            ),
            amount,
        )?;
        Ok(amount)
    }

    fn checks(&self) -> Result<()> {
        require_keys_eq!(self.a.amm_config.key(), ray::amm_config_key(), SolzError::WrongAccount);
        require_keys_eq!(self.a.ray_authority.key(), ray::authority_key(), SolzError::WrongAccount);
        require!(self.a.mine.open, SolzError::NotOpen);
        Ok(())
    }
}

pub fn buy_burn<'info>(ctx: Context<'info, Keeper<'info>>, pad: bool) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let (spend, out) = {
        let a = &*ctx.accounts;
        let c = Ctx { a, mine_key: a.mine.key(), vault_bump: a.mine.vault_bump };
        c.checks()?;
        let m = &a.mine;
        let (queue, last, gate) = if pad {
            require!(m.kind == 0, SolzError::WrongAccount);
            (m.pad_burn_queue, m.last_pad_burn, PAD_GATE_BPS)
        } else {
            (m.burn_queue, m.last_burn, GATE_BPS)
        };
        require!(queue > 0, SolzError::NothingQueued);
        require!(now >= last + COOLDOWN, SolzError::CoolingDown);
        let v = c.view()?;
        if !pad {
            // A standing bid under the mining price, kept for when it matters.
            require!(crate::state::Mine::market_price(v.sol_reserve, v.token_reserve) < m.true_cost(), SolzError::AboveTrueCost);
        }
        let cap = ((v.sol_reserve as u128) * gate / BPS) as u64;
        let spend = queue.min(cap);
        require!(spend > 0, SolzError::NothingQueued);
        let fee = ray::trade_fee_rate(&a.amm_config.to_account_info())?;
        let expect = amount_out(spend, v.sol_reserve, v.token_reserve, fee);
        require!(expect > 0, SolzError::ShortOutput);
        c.fund_wsol(c.wsol()? + spend)?;
        let out = c.buy(spend, expect - expect / 100)?;
        c.burn_tokens()?;
        (spend, out)
    };
    let a = &mut *ctx.accounts;
    let m = &mut a.mine;
    if pad {
        m.pad_burn_queue -= spend;
        m.pad_burned += out;
        m.pad_spent += spend;
        m.last_pad_burn = now;
    } else {
        m.burn_queue -= spend;
        m.total_burned += out;
        m.burn_spent += spend;
        m.last_burn = now;
    }
    let float = ray::token_amount(&a.vault_wsol.to_account_info())?;
    check_solvent(m, &a.vault.to_account_info(), float)?;
    emit!(Burned { mine: m.mint, caller: a.caller.key(), sol_spent: spend, tokens_burned: out, pad });
    Ok(())
}

pub fn deepen<'info>(ctx: Context<'info, Keeper<'info>>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let (spent, tok_added, sol_added, half) = {
        let a = &*ctx.accounts;
        let c = Ctx { a, mine_key: a.mine.key(), vault_bump: a.mine.vault_bump };
        c.checks()?;
        let m = &a.mine;
        let q = m.lp_queue;
        require!(q > 0, SolzError::NothingQueued);
        require!(now >= m.last_deepen + COOLDOWN, SolzError::CoolingDown);
        let expect_lp = anchor_spl::associated_token::get_associated_token_address(&a.vault.key(), &m.lp_mint);
        require_keys_eq!(a.vault_lp.key(), expect_lp, SolzError::WrongAccount);

        let w_start = c.wsol()?;
        let v = c.view()?;
        // The buy half is what the gate limits; the add half moves no price.
        let half = (q / 2).min(((v.sol_reserve as u128) * GATE_BPS / BPS) as u64);
        require!(half > 0, SolzError::NothingQueued);
        let fee = ray::trade_fee_rate(&a.amm_config.to_account_info())?;
        let expect = amount_out(half, v.sol_reserve, v.token_reserve, fee);
        require!(expect > 0, SolzError::ShortOutput);

        let mut funded = c.fund_wsol(half)?;
        let bought = c.buy(half, expect - expect / 100)?;

        // Add at the pool's ratio after the buy, limited by both what was
        // bought and the SOL left in the queue.
        let v2 = c.view()?;
        let avail = q - half;
        let lp_by_tok = (bought as u128) * (v2.lp_supply as u128) / (v2.token_reserve as u128);
        let lp_by_sol = (avail as u128) * (v2.lp_supply as u128) / (v2.sol_reserve as u128);
        let mut lp = lp_by_tok.min(lp_by_sol) as u64;
        lp = lp.saturating_sub(1); // Raydium rounds the amounts it takes up
        let (tok_used, sol_used) = if lp > 0 {
            let sol_need = ((lp as u128) * (v2.sol_reserve as u128)).div_ceil(v2.lp_supply as u128) as u64;
            let tok_need = ((lp as u128) * (v2.token_reserve as u128)).div_ceil(v2.lp_supply as u128) as u64;
            funded += c.fund_wsol(sol_need)?;
            let w_before = c.wsol()?;
            let t_before = c.tokens()?;
            let (max0, max1) = if m.token_is_0 { (tok_need, sol_need) } else { (sol_need, tok_need) };
            let seeds = c.vault_seeds();
            let (tv, sv) = (a.pool_token_vault.to_account_info(), a.pool_sol_vault.to_account_info());
            let (acc0, acc1, v0, v1, mint0, mint1) = if m.token_is_0 {
                (a.vault_token.to_account_info(), a.vault_wsol.to_account_info(), tv, sv, a.mint.to_account_info(), a.wsol_mint.to_account_info())
            } else {
                (a.vault_wsol.to_account_info(), a.vault_token.to_account_info(), sv, tv, a.wsol_mint.to_account_info(), a.mint.to_account_info())
            };
            ray::deposit(
                ray::DepositAccounts {
                    owner: &a.vault.to_account_info(),
                    authority: &a.ray_authority.to_account_info(),
                    pool_state: &a.pool_state.to_account_info(),
                    owner_lp_token: &a.vault_lp.to_account_info(),
                    token_0_account: &acc0,
                    token_1_account: &acc1,
                    token_0_vault: &v0,
                    token_1_vault: &v1,
                    token_program: &a.classic_token_program.to_account_info(),
                    token_program_2022: &a.token_program.to_account_info(),
                    vault_0_mint: &mint0,
                    vault_1_mint: &mint1,
                    lp_mint: &a.lp_mint.to_account_info(),
                    raydium: &a.raydium.to_account_info(),
                },
                lp,
                max0,
                max1,
                &[&seeds],
            )?;
            // Burn the LP: the liquidity can never be withdrawn.
            let lp_amount = ray::token_amount(&a.vault_lp.to_account_info())?;
            anchor_spl::token::burn(
                CpiContext::new_with_signer(
                    a.classic_token_program.key(),
                    anchor_spl::token::Burn {
                        mint: a.lp_mint.to_account_info(),
                        from: a.vault_lp.to_account_info(),
                        authority: a.vault.to_account_info(),
                    },
                    &[&seeds],
                ),
                lp_amount,
            )?;
            (t_before - c.tokens()?, w_before - c.wsol()?)
        } else {
            (0, 0)
        };
        // Whatever bought tokens did not fit the ratio are burned, never held.
        c.burn_tokens()?;
        let w_end = c.wsol()?;
        let spent = w_start + funded - w_end;
        (spent, tok_used, sol_used, half)
    };
    let a = &mut *ctx.accounts;
    let m = &mut a.mine;
    m.lp_queue -= spent;
    m.lp_sol_added += sol_added;
    m.lp_tokens_added += tok_added;
    m.last_deepen = now;
    let float = ray::token_amount(&a.vault_wsol.to_account_info())?;
    check_solvent(m, &a.vault.to_account_info(), float)?;
    emit!(Deepened { mine: m.mint, caller: a.caller.key(), sol_spent: half + sol_added, tokens_added: tok_added, sol_added });
    Ok(())
}

/// If Raydium ever switches this pool off, the queues that need it must not be stuck
/// forever. With swaps off, the burn and liquidity queues go to stakers; with only
/// deposits off, the liquidity queue joins the buy and burn. Anyone may call it, and it
/// does nothing while the pool works.
pub fn release_stuck<'info>(ctx: Context<'info, Keeper<'info>>) -> Result<()> {
    let a = &mut *ctx.accounts;
    let st = ray::status(&a.pool_state.to_account_info())?;
    let swap_off = st & 0b100 != 0;
    let deposit_off = st & 0b001 != 0;
    require!(swap_off || deposit_off, SolzError::PoolWorking);
    let float = ray::token_amount(&a.vault_wsol.to_account_info())?;
    let m = &mut a.mine;
    let lp_free = m.lp_queue.saturating_sub(float);
    if swap_off {
        require!((m.total_staked as u128) >= MIN_STAKED, SolzError::NotEnoughStaked);
        let amount = m.burn_queue + m.pad_burn_queue + lp_free;
        m.burn_queue = 0;
        m.pad_burn_queue = 0;
        m.lp_queue -= lp_free;
        distribute(m, amount);
    } else {
        m.burn_queue += lp_free;
        m.lp_queue -= lp_free;
    }
    check_solvent(m, &a.vault.to_account_info(), float)?;
    Ok(())
}
