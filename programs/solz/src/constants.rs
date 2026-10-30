use anchor_lang::prelude::*;

// ------------------------------------------------------------------ wallets
// The operations wallet and the one wallet allowed to open $SOLZ itself are
// fixed at build time. Nothing in the program can change them later.
pub const OPS: Pubkey = Pubkey::from_str_const("2VywULPtAVbeZNZmqfBzJzSrxq9NLTKTchWtCbUagngA");
pub const LAUNCHER: Pubkey = Pubkey::from_str_const("Dd2DmmxmWDDZBjFWbT9iVmcoT5ejdgmGjQCP2zFKmFj7");

// ------------------------------------------------------------------ Raydium
#[cfg(feature = "devnet")]
pub const RAYDIUM_CPMM: Pubkey = Pubkey::from_str_const("DRaycpLY18LhpbydsBWbVJtxpNv9oXPgjRSfpF2bWpYb");
#[cfg(not(feature = "devnet"))]
pub const RAYDIUM_CPMM: Pubkey = Pubkey::from_str_const("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");

pub const RAY_AUTH_SEED: &[u8] = b"vault_and_lp_mint_auth_seed";
pub const RAY_POOL_SEED: &[u8] = b"pool";
pub const RAY_LP_MINT_SEED: &[u8] = b"pool_lp_mint";
pub const RAY_VAULT_SEED: &[u8] = b"pool_vault";
pub const RAY_OBSERVATION_SEED: &[u8] = b"observation";
pub const RAY_AMM_CONFIG_SEED: &[u8] = b"amm_config";
/// Raydium CPMM fee tier used for every pool: config index 0.
pub const RAY_AMM_CONFIG_INDEX: u16 = 0;

pub const WSOL: Pubkey = Pubkey::from_str_const("So11111111111111111111111111111111111111112");

// ------------------------------------------------------------------ token
pub const DECIMALS: u8 = 9;
pub const ONE: u128 = 1_000_000_000; // one whole token in base units
/// Rates are kept in nanolamports per whole token so small launch rates stay exact.
pub const NANO: u128 = 1_000_000_000;

// ------------------------------------------------------------------ $SOLZ
pub const SOLZ_SYMBOL: &str = "SOLZ";
pub const SOLZ_NAME: &str = "SOLZ";
pub const SOLZ_CAP: u64 = 21_000_000 * 1_000_000_000;
pub const SOLZ_CYCLES: u8 = 32;
/// Headline 0.00016 SOL per $SOLZ in cycle 1. True cost is a tenth.
pub const SOLZ_RATE0: u128 = 160_000 * NANO;

// ------------------------------------------------------------------ timing
// `fast` is only for the devnet rehearsal build; mainnet uses neither feature.
#[cfg(feature = "localnet")]
pub const LOCK: i64 = 6;
#[cfg(all(feature = "fast", not(feature = "localnet")))]
pub const LOCK: i64 = 30;
#[cfg(not(any(feature = "localnet", feature = "fast")))]
pub const LOCK: i64 = 24 * 60 * 60;

#[cfg(any(feature = "localnet", feature = "fast"))]
pub const COOLDOWN: i64 = 1;
#[cfg(not(any(feature = "localnet", feature = "fast")))]
pub const COOLDOWN: i64 = 60;

// ------------------------------------------------------------------ limits
/// Smallest position that may enroll in auto mode (0.008 SOL).
pub const MIN_AUTO: u64 = 8_000_000;
/// Smallest mine (0.001 SOL). The position account's rent (about 0.0017 SOL) is paid by the
/// miner and comes back at claim.
pub const MIN_MINE: u64 = 1_000_000;
pub const MAX_NAME: usize = 32;
pub const MAX_SYMBOL: usize = 10;
pub const MAX_URI: usize = 200;
pub const MAX_FIELD: usize = 200;

// ------------------------------------------------------------------ launchpad
#[cfg(feature = "localnet")]
pub const LAUNCH_FEE: u64 = 10_000_000; // 0.01 SOL
#[cfg(not(feature = "localnet"))]
pub const LAUNCH_FEE: u64 = 80_000_000; // 0.08 SOL, all of it buys and burns $SOLZ
#[cfg(feature = "localnet")]
pub const MIN_SEED: u64 = 5_000_000;
#[cfg(not(feature = "localnet"))]
pub const MIN_SEED: u64 = 80_000_000; // 0.08 SOL

/// The pool opens at half the true cost, so the buyback works from the first mine.
pub const SEED_PRICE_DIVISOR: u128 = 20;

pub const CAPS: [u64; 3] = [
    21_000_000 * 1_000_000_000,
    100_000_000 * 1_000_000_000,
    1_000_000_000 * 1_000_000_000,
];
pub const CYCLE_PRESETS: [u8; 3] = [8, 16, 32];
/// Headline SOL it takes to mine out cycle 1, for the three size presets.
#[cfg(feature = "localnet")]
pub const SIZES: [u64; 3] = [1_000_000_000, 5_000_000_000, 21_000_000_000];
#[cfg(not(feature = "localnet"))]
pub const SIZES: [u64; 3] = [80_000_000_000, 400_000_000_000, 1_680_000_000_000];

// ------------------------------------------------------------------ splits (bps)
pub const BPS: u128 = 10_000;
pub const BPS_RETURN: u128 = 9000;
pub const BPS_LP: u128 = 400;
pub const BPS_BURN: u128 = 200;
// $SOLZ itself
pub const SOLZ_BPS_STAKE: u128 = 300;
pub const SOLZ_BPS_OPS: u128 = 100;
// launched tokens
pub const LAUNCH_BPS_STAKE: u128 = 200;
pub const LAUNCH_BPS_SOLZ: u128 = 100;
pub const LAUNCH_BPS_OPS: u128 = 50;
pub const LAUNCH_BPS_CREATOR: u128 = 50;

// ------------------------------------------------------------------ trading gates
/// Every protocol buy (liquidity, buy and burn, and the $SOLZ burner) may spend up to
/// 7.24 percent of the pool's SOL per press: about 15 percent of price impact at most.
/// Once a minute.
pub const GATE_BPS: u128 = 724;
pub const PAD_GATE_BPS: u128 = 724;

/// Dividend accumulator magnitude.
pub const MAG: u128 = 1 << 64;
/// Never divide the accumulator by less than one whole token staked.
pub const MIN_STAKED: u128 = ONE;

pub const RESERVED: [&str; 29] = [
    "SOLZ", "S0LZ", "SOL2", "ZS0L", "ZSOL", "SOL", "WSOL", "USDC", "USDT", "BONK", "JUP", "WIF", "JTO", "RAY", "PYTH",
    "ZAVA", "AVAX", "BTC", "WBTC", "ETH", "WETH", "MSOL", "JITOSOL", "BSOL", "ORCA", "POPCAT",
    "TRUMP", "PUMP", "PENGU",
];
