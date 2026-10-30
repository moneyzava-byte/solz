use super::util::*;
use super::*;
use crate::events::*;
use crate::ray;
use anchor_lang::solana_program::program::{invoke, invoke_signed};
use anchor_lang::solana_program::system_instruction;
use anchor_spl::token_2022::spl_token_2022;
use anchor_spl::token_2022::spl_token_2022::extension::ExtensionType;
use anchor_spl::token_interface::spl_token_metadata_interface::state::{Field, TokenMetadata};
use anchor_spl::token_interface::{
    metadata_pointer_initialize, token_metadata_initialize, token_metadata_update_field, MetadataPointerInitialize,
    TokenMetadataInitialize, TokenMetadataUpdateField,
};

pub const MEDIA_KEYS: [&str; 5] = ["image", "banner", "description", "x", "website"];

/// Tickers are upper case letters and digits. Reserved tickers cannot be taken.
pub fn check_symbol(symbol: &str, kind: u8) -> Result<()> {
    let b = symbol.as_bytes();
    require!(!b.is_empty() && b.len() <= MAX_SYMBOL, SolzError::BadSymbol);
    require!(b.iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()), SolzError::BadSymbol);
    if kind == 0 {
        require!(symbol == SOLZ_SYMBOL, SolzError::BadSymbol);
    } else {
        require!(!RESERVED.contains(&symbol), SolzError::SymbolTaken);
    }
    Ok(())
}

pub fn check_name(name: &str) -> Result<()> {
    let b = name.as_bytes();
    require!(!b.is_empty() && b.len() <= MAX_NAME, SolzError::BadName);
    require!(b.iter().all(|c| (0x20..=0x7e).contains(c)), SolzError::BadName);
    require!(b[0] != b' ' && b[b.len() - 1] != b' ', SolzError::BadName);
    Ok(())
}

pub fn create_mine<'info>(
    ctx: Context<'info, CreateMine<'info>>,
    kind: u8,
    name: String,
    symbol: String,
    uri: String,
    presets: [u8; 3],
    media: Vec<String>,
) -> Result<()> {
    require!(kind <= 1, SolzError::BadPreset);
    check_symbol(&symbol, kind)?;
    check_name(&name)?;
    require!(uri.len() <= MAX_URI, SolzError::TooLong);
    require!(media.len() <= MEDIA_KEYS.len(), SolzError::TooLong);
    for v in media.iter() {
        require!(v.len() <= MAX_FIELD, SolzError::TooLong);
    }
    let a = &mut *ctx.accounts;
    let rent = Rent::get()?;
    let sys = a.system_program.to_account_info();
    let creator = a.creator.to_account_info();
    // Never charge a launch fee for a pool Raydium would refuse to open.
    require!(!ray::create_disabled(&a.amm_config.to_account_info())?, SolzError::PoolsDisabled);

    // ---- terms
    let (cap, cycles, rate0) = if kind == 0 {
        require_keys_eq!(a.creator.key(), LAUNCHER, SolzError::NotLauncher);
        require_keys_eq!(a.global.solz_mine, Pubkey::default(), SolzError::AlreadyOpen);
        require!(name == SOLZ_NAME, SolzError::BadName);
        (SOLZ_CAP, SOLZ_CYCLES, SOLZ_RATE0)
    } else {
        let (ci, yi, si) = (presets[0] as usize, presets[1] as usize, presets[2] as usize);
        require!(ci < 3 && yi < 3 && si < 3, SolzError::BadPreset);
        let cap = CAPS[ci];
        // Cycle 1 is half the cap, so mining it all out costs `size` SOL headline.
        let rate0 = (SIZES[si] as u128) * 2 * ONE * NANO / (cap as u128);
        require!(rate0 > 0, SolzError::BadPreset);
        (cap, CYCLE_PRESETS[yi], rate0)
    };

    // ---- the launch fee buys and burns $SOLZ
    if kind == 1 {
        let sm = a.solz_mine.as_mut().ok_or(SolzError::WrongAccount)?;
        let sv = a.solz_vault.as_ref().ok_or(SolzError::WrongAccount)?;
        require_keys_eq!(sm.key(), a.global.solz_mine, SolzError::WrongAccount);
        let (expect_sv, _) = Pubkey::find_program_address(&[VAULT_SEED, sm.key().as_ref()], &crate::ID);
        require_keys_eq!(sv.key(), expect_sv, SolzError::WrongAccount);
        require!(sm.open, SolzError::NotOpen);
        pay_in(&creator, &sv.to_account_info(), &sys, LAUNCH_FEE)?;
        sm.pad_burn_queue += LAUNCH_FEE;
        a.global.launches += 1;
    }

    // ---- the mint: Token-2022 with its metadata stored in the mint itself
    let mint_bump = ctx.bumps.mint;
    let auth_bump = ctx.bumps.auth;
    let mint_seeds: &[&[u8]] = &[b"mint", symbol.as_bytes(), &[mint_bump]];
    let auth_seeds: &[&[u8]] = &[AUTH_SEED, &[auth_bump]];
    let auth_key = a.auth.key();
    let mint_key = a.mint.key();

    let mut meta = TokenMetadata {
        update_authority: Some(auth_key).try_into()?,
        mint: mint_key,
        name: name.clone(),
        symbol: symbol.clone(),
        uri: uri.clone(),
        additional_metadata: vec![],
    };
    for (i, v) in media.iter().enumerate() {
        if !v.is_empty() {
            meta.additional_metadata.push((MEDIA_KEYS[i].to_string(), v.clone()));
        }
    }
    let base_len = ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(&[ExtensionType::MetadataPointer])?;
    let meta_len = meta.tlv_size_of()?;
    let lamports = rent.minimum_balance(base_len + meta_len);
    // Anyone can send lamports to the mint's address before it exists, so never rely on
    // create_account: top it up, then allocate and assign, as Anchor's init does.
    let mint_info = a.mint.to_account_info();
    require_keys_eq!(*mint_info.owner, anchor_lang::system_program::ID, SolzError::SymbolTaken);
    if mint_info.lamports() < lamports {
        pay_in(&creator, &mint_info, &sys, lamports - mint_info.lamports())?;
    }
    invoke_signed(&system_instruction::allocate(&mint_key, base_len as u64), &[mint_info.clone(), sys.clone()], &[mint_seeds])?;
    invoke_signed(&system_instruction::assign(&mint_key, &spl_token_2022::ID), &[mint_info.clone(), sys.clone()], &[mint_seeds])?;
    let tp = a.token_program.to_account_info();
    metadata_pointer_initialize(
        CpiContext::new(
            tp.key(),
            MetadataPointerInitialize { token_program_id: tp.clone(), mint: a.mint.to_account_info() },
        ),
        None,
        Some(mint_key),
    )?;
    invoke(
        &spl_token_2022::instruction::initialize_mint2(&spl_token_2022::ID, &mint_key, &auth_key, None, DECIMALS)?,
        &[a.mint.to_account_info(), tp.clone()],
    )?;
    token_metadata_initialize(
        CpiContext::new_with_signer(
            tp.key(),
            TokenMetadataInitialize {
                program_id: tp.clone(),
                metadata: a.mint.to_account_info(),
                update_authority: a.auth.to_account_info(),
                mint_authority: a.auth.to_account_info(),
                mint: a.mint.to_account_info(),
            },
            &[auth_seeds],
        ),
        name.clone(),
        symbol.clone(),
        uri,
    )?;
    for (k, v) in meta.additional_metadata.iter() {
        token_metadata_update_field(
            CpiContext::new_with_signer(
                tp.key(),
                TokenMetadataUpdateField {
                    program_id: tp.clone(),
                    metadata: a.mint.to_account_info(),
                    update_authority: a.auth.to_account_info(),
                },
                &[auth_seeds],
            ),
            Field::Key(k.clone()),
            v.clone(),
        )?;
    }

    // ---- the vault: a plain system account PDA, funded with its rent
    if a.vault.lamports() == 0 {
        pay_in(&creator, &a.vault.to_account_info(), &sys, rent.minimum_balance(0))?;
    }

    // ---- the mine
    let m = &mut a.mine;
    m.kind = kind;
    m.bump = ctx.bumps.mine;
    m.vault_bump = ctx.bumps.vault;
    m.cycle = 1;
    m.cycles = cycles;
    m.mint = mint_key;
    m.creator = a.creator.key();
    m.cap = cap;
    m.max_seed = cap / 10;
    m.rate0 = rate0;

    a.symbol_entry.mint = mint_key;
    a.symbol_entry.bump = ctx.bumps.symbol_entry;
    if kind == 0 {
        a.global.solz_mine = a.mine.key();
    }

    emit!(MineCreated {
        mine: a.mine.key(),
        mint: mint_key,
        creator: a.creator.key(),
        kind,
        cap,
        cycles,
        rate0,
        name,
        symbol,
    });
    Ok(())
}

pub fn open_pool<'info>(ctx: Context<'info, OpenPool<'info>>, seed_sol: u64, fund: u64) -> Result<()> {
    let a = &mut *ctx.accounts;
    require_keys_eq!(a.creator.key(), a.mine.creator, SolzError::NotCreator);
    require!(!a.mine.open, SolzError::AlreadyOpen);
    if a.mine.kind == 1 {
        require!(seed_sol >= MIN_SEED, SolzError::SeedTooSmall);
    }
    require!(seed_sol > 0 && fund >= seed_sol, SolzError::SeedTooSmall);
    // The pool opens at half the true cost.
    let seed_tokens = ((seed_sol as u128) * ONE * NANO * SEED_PRICE_DIVISOR / a.mine.rate0) as u64;
    require!(seed_tokens > 0, SolzError::SeedTooSmall);
    require!(seed_tokens <= a.mine.max_seed, SolzError::SeedTooLarge);
    require_keys_eq!(a.amm_config.key(), ray::amm_config_key(), SolzError::WrongAccount);
    require_keys_eq!(a.ray_authority.key(), ray::authority_key(), SolzError::WrongAccount);

    let mine_key = a.mine.key();
    let vault_bump = a.mine.vault_bump;
    let vault_seeds: &[&[u8]] = &[VAULT_SEED, mine_key.as_ref(), &[vault_bump]];
    let auth_seeds: &[&[u8]] = &[AUTH_SEED, &[ctx.bumps.auth]];
    let sys = a.system_program.to_account_info();
    let vault = a.vault.to_account_info();

    // The creator funds the seed and Raydium's account rent and pool fee; the
    // vault pays Raydium and hands back whatever is left.
    let before = vault.lamports();
    pay_in(&a.creator.to_account_info(), &vault, &sys, fund)?;
    pay_out(&vault, &a.vault_wsol.to_account_info(), &sys, &mine_key, vault_bump, seed_sol)?;
    anchor_spl::token::sync_native(CpiContext::new(
        a.classic_token_program.key(),
        anchor_spl::token::SyncNative { account: a.vault_wsol.to_account_info() },
    ))?;
    anchor_spl::token_2022::mint_to(
        CpiContext::new_with_signer(
            a.token_program.key(),
            anchor_spl::token_2022::MintTo {
                mint: a.mint.to_account_info(),
                to: a.vault_token.to_account_info(),
                authority: a.auth.to_account_info(),
            },
            &[auth_seeds],
        ),
        seed_tokens,
    )?;

    let token_is_0 = a.mint.key() < WSOL;
    let (m0, m1, c0, c1, p0, p1, amt0, amt1) = if token_is_0 {
        (
            a.mint.to_account_info(), a.wsol_mint.to_account_info(),
            a.vault_token.to_account_info(), a.vault_wsol.to_account_info(),
            a.token_program.to_account_info(), a.classic_token_program.to_account_info(),
            seed_tokens, seed_sol,
        )
    } else {
        (
            a.wsol_mint.to_account_info(), a.mint.to_account_info(),
            a.vault_wsol.to_account_info(), a.vault_token.to_account_info(),
            a.classic_token_program.to_account_info(), a.token_program.to_account_info(),
            seed_sol, seed_tokens,
        )
    };
    ray::initialize(
        ray::InitAccounts {
            creator: &vault,
            amm_config: &a.amm_config.to_account_info(),
            authority: &a.ray_authority.to_account_info(),
            pool_state: &a.pool_state.to_account_info(),
            token_0_mint: &m0,
            token_1_mint: &m1,
            lp_mint: &a.lp_mint.to_account_info(),
            creator_token_0: &c0,
            creator_token_1: &c1,
            creator_lp_token: &a.vault_lp.to_account_info(),
            token_0_vault: &a.pool_vault_0.to_account_info(),
            token_1_vault: &a.pool_vault_1.to_account_info(),
            create_pool_fee: &a.create_pool_fee.to_account_info(),
            observation_state: &a.observation.to_account_info(),
            token_program: &a.classic_token_program.to_account_info(),
            token_0_program: &p0,
            token_1_program: &p1,
            associated_token_program: &a.associated_token_program.to_account_info(),
            system_program: &sys,
            rent: &a.rent.to_account_info(),
            raydium: &a.raydium.to_account_info(),
        },
        amt0,
        amt1,
        &[vault_seeds],
    )?;

    // ---- burn every LP token the pool minted to the vault
    let (v0, v1, lp_mint, _, _) = ray::pool_keys(&a.pool_state.to_account_info())?;
    require_keys_eq!(lp_mint, a.lp_mint.key(), SolzError::WrongAccount);
    let lp_amount = ray::token_amount(&a.vault_lp.to_account_info())?;
    anchor_spl::token::burn(
        CpiContext::new_with_signer(
            a.classic_token_program.key(),
            anchor_spl::token::Burn {
                mint: a.lp_mint.to_account_info(),
                from: a.vault_lp.to_account_info(),
                authority: vault.clone(),
            },
            &[vault_seeds],
        ),
        lp_amount,
    )?;

    // ---- hand back what Raydium did not need
    let after = vault.lamports();
    require!(after >= before, SolzError::Insolvent);
    pay_out(&vault, &a.creator.to_account_info(), &sys, &mine_key, vault_bump, after - before)?;

    let m = &mut a.mine;
    m.pool = a.pool_state.key();
    m.lp_mint = lp_mint;
    m.observation = a.observation.key();
    m.token_is_0 = token_is_0;
    if token_is_0 {
        m.pool_token_vault = v0;
        m.pool_sol_vault = v1;
    } else {
        m.pool_token_vault = v1;
        m.pool_sol_vault = v0;
    }
    m.total_reserved += seed_tokens;
    m.open = true;
    m.launched_at = Clock::get()?.unix_timestamp;
    let wsol_float = ray::token_amount(&a.vault_wsol.to_account_info())?;
    check_solvent(m, &vault, wsol_float)?;
    emit!(Launched { mine: m.key(), pool: m.pool, seed_sol, seed_tokens });
    Ok(())
}

pub fn set_media(ctx: Context<SetMedia>, field: String, value: String) -> Result<()> {
    require!(value.len() <= MAX_FIELD, SolzError::TooLong);
    let f = if field == "uri" {
        require!(value.len() <= MAX_URI, SolzError::TooLong);
        Field::Uri
    } else {
        require!(MEDIA_KEYS.contains(&field.as_str()), SolzError::WrongAccount);
        Field::Key(field.clone())
    };
    let a = &ctx.accounts;
    // Top the mint up for the bigger metadata; the creator pays the rent.
    let mint = a.mint.to_account_info();
    let new_len = mint.data_len() + field.len() + value.len() + 16;
    let need = Rent::get()?.minimum_balance(new_len);
    if mint.lamports() < need {
        pay_in(&a.creator.to_account_info(), &mint, &a.system_program.to_account_info(), need - mint.lamports())?;
    }
    let auth_seeds: &[&[u8]] = &[AUTH_SEED, &[ctx.bumps.auth]];
    let tp = a.token_program.to_account_info();
    token_metadata_update_field(
        CpiContext::new_with_signer(
            tp.key(),
            TokenMetadataUpdateField { program_id: tp.clone(), metadata: mint.clone(), update_authority: a.auth.to_account_info() },
            &[auth_seeds],
        ),
        f,
        value.clone(),
    )?;
    emit!(MediaSet { mine: a.mine.key(), field, value });
    Ok(())
}
