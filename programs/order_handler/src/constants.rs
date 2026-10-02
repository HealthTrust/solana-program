pub const MAX_JOB_DATA_TYPES: usize = 18;
pub const MAX_JOB_DATA_TYPE_LEN: usize = 32;
pub const MAX_PARTICIPANTS: usize = 50;
pub const MAX_FILTER_QUERY_LEN: usize = 128;
pub const MAX_RESULT_CID_LEN: usize = 64;
/// Max length of the researcher-selected algorithm id (e.g. "cohort_means").
/// An empty string means "use the TEE's default algorithm".
pub const MAX_ALGORITHM_ID_LEN: usize = 32;
/// Max length of the researcher-selected algorithm params blob (a JSON object,
/// UTF-8 encoded) the TEE decodes and passes to the algorithm's Validate/Run.
/// An empty blob means "use the algorithm's defaults".
pub const MAX_ALGORITHM_PARAMS_LEN: usize = 512;

/// How long providers have to claim their payout after a job is finalized
/// (Completed). Until every provider has claimed, the researcher may only sweep
/// the leftover escrow once this window has elapsed since `job.updated_at`.
#[cfg(not(feature = "short-timeouts"))]
pub const CLAIM_WINDOW_SECS: i64 = 30 * 24 * 60 * 60; // 30 days

/// How long a paid job (Confirmed / Executed) may sit without the TEE
/// completing it before the researcher can reclaim the escrow via
/// `refund_stuck_job`. Measured from `job.updated_at`.
#[cfg(not(feature = "short-timeouts"))]
pub const REFUND_TIMEOUT_SECS: i64 = 7 * 24 * 60 * 60; // 7 days

// TEST-ONLY values. The `short-timeouts` feature must never be enabled for a
// build that is deployed to devnet/mainnet (see scripts/run-tests.ps1).
#[cfg(feature = "short-timeouts")]
pub const CLAIM_WINDOW_SECS: i64 = 6;
#[cfg(feature = "short-timeouts")]
pub const REFUND_TIMEOUT_SECS: i64 = 3;
