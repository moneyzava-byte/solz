use crate::constants::*;
use crate::errors::SolzError;
use crate::state::Mine;
use anchor_lang::prelude::*;

impl Mine {
    /// Nanolamports per whole token in cycle `c`.
    pub fn rate_for(&self, c: u8) -> u128 {
        self.rate0 << (c.saturating_sub(1) as u32)
    }

    pub fn rate(&self) -> u128 {
        if self.cycle > self.cycles {
            self.rate_for(self.cycles)
        } else {
            self.rate_for(self.cycle)
        }
    }

    /// True cost per whole token: the rate less the 90 percent returned.
    pub fn true_cost(&self) -> u128 {
        self.rate() / 10
    }

    /// Tokens allocated to cycles 1 through `c`. The final cycle takes what is left,
    /// so it equals the one before it.
    pub fn cumulative(&self, c: u8) -> u64 {
        if c >= self.cycles {
            return self.cap;
        }
        self.cap - (self.cap >> c)
    }

    pub fn remaining_in_cycle(&self) -> u64 {
        if self.cycle > self.cycles {
            return 0;
        }
        self.cumulative(self.cycle) - self.total_reserved
    }

    pub fn bps(&self) -> Bps {
        if self.kind == 0 {
            Bps { lp: BPS_LP, burn: BPS_BURN, stake: SOLZ_BPS_STAKE, solz: 0, ops: SOLZ_BPS_OPS, creator: 0 }
        } else {
            Bps {
                lp: BPS_LP,
                burn: BPS_BURN,
                stake: LAUNCH_BPS_STAKE,
                solz: LAUNCH_BPS_SOLZ,
                ops: LAUNCH_BPS_OPS,
                creator: LAUNCH_BPS_CREATOR,
            }
        }
    }

    /// Prices `amount` lamports across cycle boundaries. Returns the tokens
    /// promised and the lamports actually used (less than `amount` only when
    /// the last cycle closes under this commit).
    pub fn price(&mut self, amount: u64) -> Result<(u64, u64)> {
        let mut remaining = amount as u128;
        let mut used: u128 = 0;
        let mut reward: u128 = 0;
        while remaining > 0 {
            if self.cycle > self.cycles {
                break;
            }
            let r = self.rate_for(self.cycle);
            let room = (self.cumulative(self.cycle) - self.total_reserved) as u128;
            let want = remaining
                .checked_mul(ONE * NANO)
                .ok_or(SolzError::Overflow)?
                / r;
            if want < room {
                reward += want;
                self.total_reserved += want as u64;
                used += remaining;
                remaining = 0;
            } else {
                reward += room;
                self.total_reserved += room as u64;
                // Lamports that buy exactly `room`, rounded up.
                let cost = (room * r + ONE * NANO - 1) / (ONE * NANO);
                let cost = cost.min(remaining);
                used += cost;
                remaining -= cost;
                self.cycle += 1;
                emit!(crate::events::CycleAdvanced { mine: self.mint, cycle: self.cycle });
            }
        }
        if used == 0 && amount > 0 {
            return err!(SolzError::MiningComplete);
        }
        Ok((reward as u64, used as u64))
    }

    /// Market price in nanolamports per whole token.
    pub fn market_price(sol_res: u64, tok_res: u64) -> u128 {
        if tok_res == 0 {
            return u128::MAX;
        }
        (sol_res as u128) * ONE * NANO / (tok_res as u128)
    }
}

pub struct Bps {
    pub lp: u128,
    pub burn: u128,
    pub stake: u128,
    pub solz: u128,
    pub ops: u128,
    pub creator: u128,
}

pub fn bps_of(amount: u64, bps: u128) -> u64 {
    ((amount as u128) * bps / BPS) as u64
}

/// Constant product output with the pool's trade fee (fee rate over 1_000_000).
pub fn amount_out(amount_in: u64, r_in: u64, r_out: u64, fee_rate: u64) -> u64 {
    let fee = ((amount_in as u128) * (fee_rate as u128) + 999_999) / 1_000_000;
    let in_less = (amount_in as u128).saturating_sub(fee);
    if in_less == 0 {
        return 0;
    }
    (in_less * (r_out as u128) / ((r_in as u128) + in_less)) as u64
}
