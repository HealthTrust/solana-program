pub const MAX_DATA_TYPES: usize = 18;
pub const MAX_DATA_TYPE_LEN: usize = 32;
pub const MAX_CHRONIC_CONDITIONS: usize = 16;

pub const MAX_DEVICE_TYPE_LEN: usize = 32;
pub const MAX_DEVICE_MODEL_LEN: usize = 48;
pub const MAX_SERVICE_PROVIDER_LEN: usize = 48;

pub const MAX_CID_LEN: usize = 64;

// Quality digest (event-only, carried on update_upload_unit). See
// QUALITY_DIGEST_DESIGN.md §4/§5 and COHORT_IMPL_CONTRACT.md §3.
//
// Tracks MAX_DATA_TYPES: a unit carrying N distinct signals must be able to
// digest all N, otherwise the missing ones preflight-score q_i = 0.
//
// NOTE: the digest travels as instruction data, so it is bounded by the 1232-byte
// transaction limit, not by account space. 18 signals only fit when signal names
// are short (see the serialized_digest_fits_in_a_transaction test); a full 18
// signals at MAX_SIGNAL_NAME_LEN each does NOT fit in one transaction.
pub const MAX_QUALITY_SIGNALS: usize = 18;
pub const MAX_SIGNAL_NAME_LEN: usize = 32;
pub const MAX_DIGEST_VERSION_LEN: usize = 16;
