use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("DMbLxuGRQdtYwhsXTGdp1qAKbzzR7jiR3gvvttgU36Tj");

pub const OAPP_SEED: &[u8] = b"OApp";
pub const PEER_SEED: &[u8] = b"Peer";
pub const TRIAL_SEED: &[u8] = b"Trial";
pub const VAULT_SEED: &[u8] = b"Vault";

pub const MAX_TITLE: usize = 64;
pub const MAX_DESC: usize = 256;
pub const MAX_CATEGORY: usize = 32;
pub const MAX_SKILLS: usize = 128;
pub const MAX_DIFFICULTY: usize = 16;
pub const MAX_OBJECTIVE: usize = 256;
pub const MAX_REQUIREMENTS: usize = 512;
pub const MAX_DOD: usize = 512;
pub const REFERRAL_SEED: &[u8] = b"Referral";
pub const MAX_NOTE: usize = 128;

#[program]
pub mod outcom {
    use super::*;

    pub fn init_oapp(
        ctx: Context<InitOApp>,
        endpoint_program: Pubkey,
        admin: Pubkey,
    ) -> Result<()> {
        let oapp_config = &mut ctx.accounts.oapp_config;
        oapp_config.admin = admin;
        oapp_config.endpoint_program = endpoint_program;
        oapp_config.bump = ctx.bumps.oapp_config;
        Ok(())
    }

    pub fn set_peer(ctx: Context<SetPeer>, src_eid: u32, peer_address: [u8; 32]) -> Result<()> {
        let peer = &mut ctx.accounts.peer_config;
        peer.src_eid = src_eid;
        peer.address = peer_address;
        peer.bump = ctx.bumps.peer_config;
        Ok(())
    }

    pub fn initialize_trial(
        ctx: Context<InitializeTrial>,
        trial_id: String,
        title: String,
        description: String,
        category: String,
        skills: String,
        difficulty: String,
        objective: String,
        requirements: String,
        candidate_reward: u64,
        referral_reward: u64,
    ) -> Result<()> {
        require!(trial_id.len() <= 32, CustomError::TrialIdTooLong);
        require!(title.len() <= MAX_TITLE, CustomError::MetadataTooLong);
        require!(description.len() <= MAX_DESC, CustomError::MetadataTooLong);
        require!(category.len() <= MAX_CATEGORY, CustomError::MetadataTooLong);
        require!(skills.len() <= MAX_SKILLS, CustomError::MetadataTooLong);
        require!(
            difficulty.len() <= MAX_DIFFICULTY,
            CustomError::MetadataTooLong
        );
        require!(
            objective.len() <= MAX_OBJECTIVE,
            CustomError::MetadataTooLong
        );
        require!(
            requirements.len() <= MAX_REQUIREMENTS,
            CustomError::MetadataTooLong
        );

        let trial = &mut ctx.accounts.trial_account;
        trial.employer = ctx.accounts.employer.key();
        trial.trial_id = trial_id;
        trial.title = title;
        trial.description = description;
        trial.category = category;
        trial.skills = skills;
        trial.difficulty = difficulty;
        trial.objective = objective;
        trial.requirements = requirements;
        trial.candidate_reward = candidate_reward;
        trial.referral_reward = referral_reward;
        trial.total_locked = candidate_reward
            .checked_add(referral_reward)
            .ok_or(CustomError::Overflow)?;
        trial.status = TrialStatus::Open;
        trial.selected_candidate = None;
        trial.vault_bump = ctx.bumps.vault_account;

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.employer_token_account.to_account_info(),
                    to: ctx.accounts.vault_account.to_account_info(),
                    authority: ctx.accounts.employer.to_account_info(),
                },
            ),
            trial.total_locked,
        )?;
        Ok(())
    }

    pub fn start_trial(ctx: Context<StartTrial>) -> Result<()> {
        let trial = &mut ctx.accounts.trial_account;
        require!(trial.status == TrialStatus::Open, CustomError::TrialNotOpen);

        trial.selected_candidate = Some(ctx.accounts.candidate.key());
        trial.status = TrialStatus::InProgress;
        Ok(())
    }

    pub fn refer_candidate(
        ctx: Context<ReferCandidate>,
        candidate: Pubkey,
        note: String,
    ) -> Result<()> {
        require!(note.len() <= MAX_NOTE, CustomError::MetadataTooLong);
        require!(
            ctx.accounts.trial_account.status == TrialStatus::Open
                || ctx.accounts.trial_account.status == TrialStatus::InProgress,
            CustomError::InvalidStatus
        );
        require!(
            candidate != ctx.accounts.referrer.key(),
            CustomError::CannotReferSelf
        );

        let referral = &mut ctx.accounts.referral;
        referral.trial = ctx.accounts.trial_account.key();
        referral.trial_id = ctx.accounts.trial_account.trial_id.clone();
        referral.referrer = ctx.accounts.referrer.key();
        referral.candidate = candidate;
        referral.note = note;
        Ok(())
    }

    pub fn lz_receive(ctx: Context<LzReceive>, params: LzReceiveParams) -> Result<()> {
        let oapp_config = &ctx.accounts.oapp_config;
        let peer_config = &ctx.accounts.peer_config;

        require_keys_eq!(
            ctx.accounts.endpoint_program.key(),
            oapp_config.endpoint_program,
            CustomError::UnauthorizedEndpoint
        );
        require_eq!(
            params.src_eid,
            peer_config.src_eid,
            CustomError::InvalidSourceEID
        );
        require!(
            params.sender == peer_config.address,
            CustomError::InvalidPeerSender
        );

        let payload = &params.payload;
        require!(payload.len() >= 105, CustomError::InvalidPayloadLength);

        let trial_id_bytes = &payload[0..32];
        let trial_id = String::from_utf8_lossy(trial_id_bytes)
            .trim_matches('\0')
            .to_string();
        let candidate_bytes: [u8; 32] = payload[32..64]
            .try_into()
            .map_err(|_| CustomError::InvalidPayload)?;
        let referrer_bytes: [u8; 32] = payload[64..96]
            .try_into()
            .map_err(|_| CustomError::InvalidPayload)?;

        let score_bytes: [u8; 8] = payload[96..104]
            .try_into()
            .map_err(|_| CustomError::InvalidPayload)?;
        let score = u64::from_be_bytes(score_bytes);
        let is_verified = payload[104] != 0;

        let candidate_pubkey = Pubkey::new_from_array(candidate_bytes);
        let referrer_pubkey = Pubkey::new_from_array(referrer_bytes);

        let trial = &mut ctx.accounts.trial_account;
        require!(trial.trial_id == trial_id, CustomError::TrialIdMismatch);
        require!(
            trial.status == TrialStatus::InProgress || trial.status == TrialStatus::UnderReview,
            CustomError::InvalidStatus
        );

        if !is_verified || score < 70 {
            trial.status = TrialStatus::Rejected;
            return Ok(());
        }

        let trial_key = trial.key();
        let seeds = &[VAULT_SEED, trial_key.as_ref(), &[trial.vault_bump]];
        let signer_seeds = &[&seeds[..]];

        require_keys_eq!(
            ctx.accounts.candidate_token_account.owner,
            candidate_pubkey,
            CustomError::InvalidCandidateAccount
        );
        let candidate_cpi = Transfer {
            from: ctx.accounts.vault_account.to_account_info(),
            to: ctx.accounts.candidate_token_account.to_account_info(),
            authority: ctx.accounts.vault_account.to_account_info(),
        };
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                candidate_cpi,
                signer_seeds,
            ),
            trial.candidate_reward,
        )?;

        if trial.referral_reward > 0 && ctx.accounts.referrer_token_account.is_some() {
            let ref_acc = ctx.accounts.referrer_token_account.as_ref().unwrap();
            require_keys_eq!(
                ref_acc.owner,
                referrer_pubkey,
                CustomError::InvalidReferrerAccount
            );

            let referral_cpi = Transfer {
                from: ctx.accounts.vault_account.to_account_info(),
                to: ref_acc.to_account_info(),
                authority: ctx.accounts.vault_account.to_account_info(),
            };
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    referral_cpi,
                    signer_seeds,
                ),
                trial.referral_reward,
            )?;
        }

        trial.status = TrialStatus::Paid;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitOApp<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = 8 + OAppConfig::SIZE,
        seeds = [OAPP_SEED],
        bump
    )]
    pub oapp_config: Account<'info, OAppConfig>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(src_eid: u32)]
pub struct SetPeer<'info> {
    #[account(mut, address = oapp_config.admin)]
    pub admin: Signer<'info>,
    pub oapp_config: Account<'info, OAppConfig>,
    #[account(
        init_if_needed,
        payer = admin,
        space = 8 + PeerConfig::SIZE,
        seeds = [PEER_SEED, oapp_config.key().as_ref(), &src_eid.to_be_bytes()],
        bump
    )]
    pub peer_config: Account<'info, PeerConfig>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(trial_id: String)]
pub struct InitializeTrial<'info> {
    #[account(mut)]
    pub employer: Signer<'info>,
    #[account(
        init,
        payer = employer,
        space = 8 + TrialAccount::SIZE,
        seeds = [TRIAL_SEED, employer.key().as_ref(), trial_id.as_bytes()],
        bump
    )]
    pub trial_account: Account<'info, TrialAccount>,
    #[account(
        init,
        payer = employer,
        seeds = [VAULT_SEED, trial_account.key().as_ref()],
        bump,
        token::mint = usdc_mint,
        token::authority = vault_account
    )]
    pub vault_account: Account<'info, TokenAccount>,
    pub usdc_mint: Account<'info, Mint>,
    #[account(mut)]
    pub employer_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct StartTrial<'info> {
    #[account(mut)]
    pub candidate: Signer<'info>,
    #[account(mut)]
    pub trial_account: Account<'info, TrialAccount>,
}

#[derive(Accounts)]
#[instruction(params: LzReceiveParams)]
pub struct LzReceive<'info> {
    pub endpoint_program: Signer<'info>,
    #[account(seeds = [OAPP_SEED], bump = oapp_config.bump)]
    pub oapp_config: Account<'info, OAppConfig>,
    #[account(seeds = [PEER_SEED, oapp_config.key().as_ref(), &params.src_eid.to_be_bytes()], bump = peer_config.bump)]
    pub peer_config: Account<'info, PeerConfig>,
    #[account(mut)]
    pub trial_account: Account<'info, TrialAccount>,
    #[account(mut, seeds = [VAULT_SEED, trial_account.key().as_ref()], bump = trial_account.vault_bump)]
    pub vault_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub candidate_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub referrer_token_account: Option<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(candidate: Pubkey)]
pub struct ReferCandidate<'info> {
    #[account(mut)]
    pub referrer: Signer<'info>,
    pub trial_account: Account<'info, TrialAccount>,
    #[account(
        init,
        payer = referrer,
        space = 8 + Referral::SIZE,
        seeds = [REFERRAL_SEED, trial_account.key().as_ref(), candidate.as_ref()],
        bump
    )]
    pub referral: Account<'info, Referral>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct Referral {
    pub trial: Pubkey,
    pub trial_id: String,
    pub referrer: Pubkey,
    pub candidate: Pubkey,
    pub note: String,
}

impl Referral {
    pub const SIZE: usize = 32 + (4 + 32) + 32 + 32 + (4 + MAX_NOTE);
}


#[account]
pub struct OAppConfig {
    pub admin: Pubkey,
    pub endpoint_program: Pubkey,
    pub bump: u8,
}

impl OAppConfig {
    pub const SIZE: usize = 32 + 32 + 1;
}

#[account]
pub struct PeerConfig {
    pub src_eid: u32,
    pub address: [u8; 32],
    pub bump: u8,
}

impl PeerConfig {
    pub const SIZE: usize = 4 + 32 + 1;
}

#[account]
pub struct TrialAccount {
    pub employer: Pubkey,
    pub trial_id: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub skills: String,
    pub difficulty: String,
    pub objective: String,
    pub requirements: String,
    pub candidate_reward: u64,
    pub referral_reward: u64,
    pub total_locked: u64,
    pub status: TrialStatus,
    pub selected_candidate: Option<Pubkey>,
    pub vault_bump: u8,
}

impl TrialAccount {
    pub const SIZE: usize = 32
        + (4 + 32)
        + (4 + MAX_TITLE)
        + (4 + MAX_DESC)
        + (4 + MAX_CATEGORY)
        + (4 + MAX_SKILLS)
        + (4 + MAX_DIFFICULTY)
        + (4 + MAX_OBJECTIVE)
        + (4 + MAX_REQUIREMENTS)
        + 8
        + 8
        + 8
        + 1
        + (1 + 32)
        + 1;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum TrialStatus {
    Open,
    InProgress,
    ReadyTorun,
    UnderReview,
    Verified,
    Paid,
    Rejected,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct LzReceiveParams {
    pub src_eid: u32,
    pub sender: [u8; 32],
    pub nonce: u64,
    pub guid: [u8; 32],
    pub payload: Vec<u8>,
    pub extra_data: Vec<u8>,
}

#[error_code]
pub enum CustomError {
    #[msg("Trial ID exceeds maximum length of 32 bytes.")]
    TrialIdTooLong,
    #[msg("Trial is not open for applicants.")]
    TrialNotOpen,
    #[msg("Trial is in an invalid state for this operation.")]
    InvalidStatus,
    #[msg("Arithmetic overflow occurred.")]
    Overflow,
    #[msg("Unauthorized LayerZero Endpoint caller.")]
    UnauthorizedEndpoint,
    #[msg("Invalid Source EID from LayerZero message.")]
    InvalidSourceEID,
    #[msg("Invalid Peer Sender from LayerZero message.")]
    InvalidPeerSender,
    #[msg("Invalid payload length.")]
    InvalidPayloadLength,
    #[msg("Invalid payload data.")]
    InvalidPayload,
    #[msg("Trial ID in payload does not match account.")]
    TrialIdMismatch,
    #[msg("Candidate token account does not match payload recipient.")]
    InvalidCandidateAccount,
    #[msg("Referrer token account does not match payload recipient.")]
    InvalidReferrerAccount,
    #[msg("A metadata field exceeds its maximum length.")]
    MetadataTooLong,
    #[msg("Referrer cannot refer themselves.")]
    CannotReferSelf,
}
