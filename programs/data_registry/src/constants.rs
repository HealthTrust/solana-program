pub const MAX_DATA_TYPES: usize = 8;
pub const MAX_DATA_TYPE_LEN: usize = 32;
pub const MAX_CHRONIC_CONDITIONS: usize = 16;

pub const MAX_DEVICE_TYPE_LEN: usize = 32;
pub const MAX_DEVICE_MODEL_LEN: usize = 48;
pub const MAX_SERVICE_PROVIDER_LEN: usize = 48;

pub const MAX_CID_LEN: usize = 64;

// Quality digest (event-only, carried on update_upload_unit). See
// QUALITY_DIGEST_DESIGN.md §4/§5 and COHORT_IMPL_CONTRACT.md §3.
pub const MAX_QUALITY_SIGNALS: usize = 8;
pub const MAX_SIGNAL_NAME_LEN: usize = 32;
pub const MAX_DIGEST_VERSION_LEN: usize = 16;
