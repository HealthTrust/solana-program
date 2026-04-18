use anchor_lang::prelude::*;

#[error_code]
pub enum RegistryError {
    #[msg("Registry is paused")]
    Paused,
    #[msg("Caller is not the registry owner")]
    Unauthorized,
    #[msg("Caller is not the meta entry owner")]
    NotOwner,
    #[msg("Caller is not the TEE authority")]
    NotTeeAuthority,
    #[msg("data_type_hashes must not be empty")]
    EmptyDataTypes,
    #[msg("Too many data types (max 8)")]
    TooManyDataTypes,
    #[msg("Too many chronic conditions (max 16)")]
    TooManyConditions,
    #[msg("day_end_timestamp must be greater than day_start_timestamp")]
    InvalidTimestampRange,
    #[msg("feat_cid must not be empty")]
    EmptyFeatCid,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("New owner cannot be the zero address")]
    InvalidOwner,
    #[msg("TEE authority cannot be the zero address")]
    InvalidAuthority,
}
