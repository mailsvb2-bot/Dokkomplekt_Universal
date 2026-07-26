pub type CoreResult<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    BadJson(String),
    BadCanonicalJson(String),
    MissingProof,
    BadProof,
    BadPublicKey,
    MachineMismatch,
    NotYetValid,
    Expired,
    UnknownPlan(String),
    BadUsageLedger(String),
    ClockRollback,
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for CoreError {}
