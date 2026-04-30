pub mod compliance;
pub mod config;
pub mod domain;
pub mod events;
pub mod feed_ingestion;
pub mod live_beta_readback;
pub mod live_beta_signing;
pub mod market_discovery;
pub mod metrics;
pub mod normalization;
pub mod paper_executor;
pub mod reference_feed;
pub mod replay;
pub mod reporting;
pub mod risk_engine;
pub mod safety;
pub mod secret_handling;
pub mod shutdown;
pub mod signal_engine;
pub mod state;
pub mod storage;

pub fn module_names() -> Vec<&'static str> {
    vec![
        compliance::MODULE,
        config::MODULE,
        domain::MODULE,
        events::MODULE,
        feed_ingestion::MODULE,
        live_beta_readback::MODULE,
        live_beta_signing::MODULE,
        market_discovery::MODULE,
        metrics::MODULE,
        normalization::MODULE,
        paper_executor::MODULE,
        reference_feed::MODULE,
        replay::MODULE,
        reporting::MODULE,
        risk_engine::MODULE,
        safety::MODULE,
        secret_handling::MODULE,
        shutdown::MODULE,
        signal_engine::MODULE,
        state::MODULE,
        storage::MODULE,
    ]
}
