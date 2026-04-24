use anchor_lang::prelude::*;

#[error_code]
pub enum OrderError {
    #[msg("Caller is not the order handler owner")]
    Unauthorized,
    #[msg("Caller is not the ROFL authority")]
    NotRoflAuthority,
    #[msg("Job status does not allow this operation")]
    InvalidStatus,
    #[msg("Job ID in instruction does not match account")]
    JobIdMismatch,
    #[msg("num_days must be greater than zero")]
    InvalidNumDays,
    #[msg("template_id must be greater than zero")]
    InvalidTemplateId,
    #[msg("data_types must not be empty")]
    EmptyDataTypes,
    #[msg("Too many data types (max 8)")]
    TooManyDataTypes,
    #[msg("max_participants must be greater than zero")]
    InvalidMaxParticipants,
    #[msg("Too many selected participants (max 50)")]
    TooManyParticipants,
    #[msg("Payment amount is less than the required final_total")]
    InsufficientPayment,
    #[msg("final_total must be greater than zero before payment")]
    ZeroFinalTotal,
    #[msg("result_cid must not be empty")]
    EmptyResultCid,
    #[msg("No participants to distribute payout to")]
    NoParticipants,
    #[msg("Escrow vault has insufficient lamports")]
    InsufficientEscrow,
    #[msg("Computed amount_per_provider is zero")]
    ZeroAmountPerProvider,
    #[msg("Signer is not a selected participant for this job")]
    NotAParticipant,
    #[msg("Participant index exceeds bitmap capacity")]
    ParticipantIndexOutOfRange,
    #[msg("This provider has already claimed their payout")]
    AlreadyClaimed,
    #[msg("Job can only be cancelled in PENDING_PREFLIGHT or AWAITING_CONFIRMATION")]
    CannotCancelAtThisStage,
    #[msg("Sweep not allowed: not all providers have claimed and caller is not the researcher")]
    SweepNotAllowed,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("New owner cannot be the zero address")]
    InvalidOwner,
    #[msg("ROFL authority cannot be the zero address")]
    InvalidAuthority,
}
