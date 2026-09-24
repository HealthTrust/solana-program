use anchor_lang::prelude::*;

#[event]
pub struct MetaEntryCreated {
    pub meta_id: u64,
    pub owner: Pubkey,
    pub data_types: Vec<String>,
    pub total_duration: u64,
    pub date_of_creation: i64,
}

/// Emitted alongside `MetaEntryCreated`. Replaces the former `MetaAttributes`
/// event: instead of plaintext personal attributes, the upload carries only the
/// salted commitment to the provider's off-chain profile version
/// (`sha256(salt || canonical_profile)`). The indexer stores it in
/// `meta_commit` and the backend flags each meta's `integrity` by comparing it
/// with the profile version it serves. See MVP/OFFCHAIN_ATTRIBUTES_DESIGN.md.
#[event]
pub struct MetaCommit {
    pub meta_id: u64,
    pub owner: Pubkey,
    pub profile_commit: [u8; 32],
}

#[event]
pub struct MetaDeviceInfo {
    pub meta_id: u64,
    pub device_type: String,
    pub device_model: String,
    pub service_provider: String,
}

/// Emitted by `update_meta_data_types` (issue #44). `data_types` is the full
/// resulting declared set — indexers should treat it as authoritative and
/// overwrite, not merge. `added` is the delta this call contributed and is
/// empty on an idempotent no-op call.
#[event]
pub struct MetaDataTypesUpdated {
    pub meta_id: u64,
    pub owner: Pubkey,
    pub data_types: Vec<String>,
    pub added: Vec<String>,
    pub timestamp: i64,
}

#[event]
pub struct UploadUnitCreated {
    pub meta_id: u64,
    pub unit_index: u32,
    pub raw_cid: String,
    pub day_start_timestamp: i64,
    pub day_end_timestamp: i64,
    pub date_of_creation: i64,
}

#[event]
pub struct DataStored {
    pub meta_id: u64,
    pub unit_index: u32,
    pub owner: Pubkey,
    pub raw_cid: String,
    pub day_start_timestamp: i64,
    pub day_end_timestamp: i64,
    pub total_duration: u64,
    pub added_duration: u64,
}

/// Per-signal quality aggregate inputs, computed by the TEE at clean time and
/// carried on the `update_upload_unit` transaction. Event-only — NOT stored in
/// account state (see QUALITY_DIGEST_DESIGN.md §4/§5). The read-time scorer
/// applies `numDays` + the blend to these inputs; only the inputs travel here.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SignalQuality {
    pub signal: String,
    pub valid_samples: u32,
    pub total_samples: u32,
    pub outlier_count: u32,
    pub longest_gap_seconds: u32,
    pub first_ts: i64,
    pub last_ts: i64,
}

#[event]
pub struct DataEntryVersionUpdated {
    pub meta_id: u64,
    pub unit_index: u32,
    pub feat_cid: String,
    pub updater: Pubkey,
    pub timestamp: i64,
    // Quality digest (appended; legacy events lack these — indexer dual-decodes).
    pub quality: Vec<SignalQuality>,
    pub signal_table_version: String,
    pub extraction_version: String,
    pub digest_schema_version: u8,
}

#[event]
pub struct DataEntryDeleted {
    pub meta_id: u64,
    pub owner: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct UploadUnitClosed {
    pub meta_id: u64,
    pub unit_index: u32,
}

#[event]
pub struct PausedStateChanged {
    pub paused: bool,
}

#[event]
pub struct OwnershipTransferred {
    pub previous: Pubkey,
    pub new: Pubkey,
}

#[event]
pub struct EmergencyWithdrawal {
    pub recipient: Pubkey,
    pub amount: u64,
}
