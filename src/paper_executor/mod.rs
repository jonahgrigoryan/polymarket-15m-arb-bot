pub mod lifecycle;
pub mod pnl;

pub use lifecycle::{
    FillSimulationInput, PaperExecutionAuditEvent, PaperExecutionError, PaperExecutionResult,
    PaperExecutor, PaperExecutorConfig,
};
pub use pnl::{
    fee_paid, mark_position, MarketSettlement, MarketSettlementOutcome, PaperPositionBook,
    PositionKey, PositionUpdate, SettlementUpdate,
};

pub const MODULE: &str = "paper_executor";
