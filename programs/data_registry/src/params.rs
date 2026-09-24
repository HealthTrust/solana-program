use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UploadNewMetaParams {
    pub raw_cid: String,
    pub data_types: Vec<String>,
    pub device_type: String,
    pub device_model: String,
    pub service_provider: String,
    pub day_start_timestamp: i64,
    pub day_end_timestamp: i64,
    /// `sha256(salt || canonical_profile)` returned by the backend's
    /// `PUT /profile` for the provider's current attribute version. Personal
    /// attributes themselves are never part of the upload (see `DataEntryMeta`).
    pub profile_commit: [u8; 32],
}
