use anchor_lang::prelude::*;

#[error_code]
pub enum SolzError {
    #[msg("Mining is not open")]
    NotOpen,
    #[msg("Already open")]
    AlreadyOpen,
    #[msg("Only the launcher may do this")]
    NotLauncher,
    #[msg("Only the creator may do this")]
    NotCreator,
    #[msg("Every cycle is mined out")]
    MiningComplete,
    #[msg("Amount is too small")]
    TooSmall,
    #[msg("Seed is too large")]
    SeedTooLarge,
    #[msg("Seed is too small")]
    SeedTooSmall,
    #[msg("Nothing has matured")]
    NothingMatured,
    #[msg("Position has not matured")]
    NotMatured,
    #[msg("Auto has ended for this position")]
    AutoEnded,
    #[msg("Bad auto through cycle")]
    BadAutoThrough,
    #[msg("Auto needs at least 0.008 SOL")]
    BelowAutoMinimum,
    #[msg("Nothing to withdraw")]
    NothingToWithdraw,
    #[msg("Cooling down")]
    CoolingDown,
    #[msg("Price is at or above the true cost")]
    AboveTrueCost,
    #[msg("Nothing queued")]
    NothingQueued,
    #[msg("Bad preset")]
    BadPreset,
    #[msg("Bad name")]
    BadName,
    #[msg("Bad symbol")]
    BadSymbol,
    #[msg("Symbol is taken")]
    SymbolTaken,
    #[msg("Text is too long")]
    TooLong,
    #[msg("Wrong account")]
    WrongAccount,
    #[msg("Not enough staked")]
    NotEnoughStaked,
    #[msg("Math overflow")]
    Overflow,
    #[msg("Pool returned too little")]
    ShortOutput,
    #[msg("Vault would be short")]
    Insolvent,
    #[msg("Not the owner of this position")]
    NotOwner,
    #[msg("Staked less than 24 hours ago")]
    StakeLocked,
    #[msg("The pool is working; nothing to release")]
    PoolWorking,
    #[msg("Raydium is not taking new pools right now")]
    PoolsDisabled,
}
