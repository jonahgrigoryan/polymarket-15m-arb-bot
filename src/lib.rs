pub mod compliance;
pub mod config;
pub mod domain;
pub mod events;
pub mod execution_intent;
pub mod feed_ingestion;
pub mod live_alpha_config;
pub mod live_alpha_gate;
pub mod live_alpha_metrics;
pub mod live_alpha_preflight;
pub mod live_balance_tracker;
pub mod live_beta_canary;
pub mod live_beta_cancel;
pub mod live_beta_order_lifecycle;
pub mod live_beta_readback;
pub mod live_beta_signing;
pub mod live_executor;
pub mod live_fill_canary;
pub mod live_heartbeat;
pub mod live_maker_micro;
pub mod live_order_journal;
pub mod live_position_book;
pub mod live_quote_manager;
pub mod live_reconciliation;
pub mod live_risk_engine;
pub mod live_startup_recovery;
pub mod live_user_events;
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
        execution_intent::MODULE,
        feed_ingestion::MODULE,
        live_alpha_config::MODULE,
        live_alpha_gate::MODULE,
        live_alpha_metrics::MODULE,
        live_alpha_preflight::MODULE,
        live_fill_canary::MODULE,
        live_balance_tracker::MODULE,
        live_beta_canary::MODULE,
        live_beta_cancel::MODULE,
        live_beta_order_lifecycle::MODULE,
        live_beta_readback::MODULE,
        live_beta_signing::MODULE,
        live_executor::MODULE,
        live_heartbeat::MODULE,
        live_maker_micro::MODULE,
        live_order_journal::MODULE,
        live_position_book::MODULE,
        live_quote_manager::MODULE,
        live_reconciliation::MODULE,
        live_risk_engine::MODULE,
        live_startup_recovery::MODULE,
        live_user_events::MODULE,
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
