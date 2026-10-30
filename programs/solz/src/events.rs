use anchor_lang::prelude::*;

#[event]
pub struct MineCreated {
    pub mine: Pubkey,
    pub mint: Pubkey,
    pub creator: Pubkey,
    pub kind: u8,
    pub cap: u64,
    pub cycles: u8,
    pub rate0: u128,
    pub name: String,
    pub symbol: String,
}

#[event]
pub struct Launched {
    pub mine: Pubkey,
    pub pool: Pubkey,
    pub seed_sol: u64,
    pub seed_tokens: u64,
}

#[event]
pub struct Mined {
    pub mine: Pubkey,
    pub owner: Pubkey,
    pub position: Pubkey,
    pub amount: u64,
    pub reward: u64,
    pub cycle_after: u8,
    pub auto_through: u8,
    pub rolled: bool,
}

#[event]
pub struct Claimed {
    pub mine: Pubkey,
    pub owner: Pubkey,
    pub principal: u64,
    pub reward: u64,
}

#[event]
pub struct CycleAdvanced {
    pub mine: Pubkey,
    pub cycle: u8,
}

#[event]
pub struct AutoSet {
    pub mine: Pubkey,
    pub position: Pubkey,
    pub auto_through: u8,
}

#[event]
pub struct Burned {
    pub mine: Pubkey,
    pub caller: Pubkey,
    pub sol_spent: u64,
    pub tokens_burned: u64,
    pub pad: bool,
}

#[event]
pub struct Deepened {
    pub mine: Pubkey,
    pub caller: Pubkey,
    pub sol_spent: u64,
    pub tokens_added: u64,
    pub sol_added: u64,
}

#[event]
pub struct Staked {
    pub mine: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
    pub total: u64,
}

#[event]
pub struct Unstaked {
    pub mine: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
    pub total: u64,
}

#[event]
pub struct DividendsPaid {
    pub mine: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
}

#[event]
pub struct Withdrawn {
    pub mine: Pubkey,
    pub to: Pubkey,
    pub amount: u64,
    pub what: u8, // 0 ops, 1 creator, 2 refund
}

#[event]
pub struct MediaSet {
    pub mine: Pubkey,
    pub field: String,
    pub value: String,
}
