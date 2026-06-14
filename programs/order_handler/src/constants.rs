pub const MAX_JOB_DATA_TYPES: usize = 8;
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
