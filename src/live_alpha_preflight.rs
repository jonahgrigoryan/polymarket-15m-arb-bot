use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const MODULE: &str = "live_alpha_preflight";
pub const POLYMARKET_CRYPTO_TAKER_FEE_RATE: f64 = 0.072;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveAlphaPreflightMode {
    ReadOnly,
    DryRun,
    FinalSubmit,
}

impl LiveAlphaPreflightMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::DryRun => "dry_run",
            Self::FinalSubmit => "final_submit",
        }
    }

    fn requires_submit_gates(self) -> bool {
        matches!(self, Self::FinalSubmit)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveAlphaApprovedOrderBounds {
    pub approval_id: String,
    pub approved_host_ids: Vec<String>,
    pub wallet_id: String,
    pub funder_id: String,
    pub signature_type: String,
    pub market_slug: String,
    pub condition_id: String,
    pub token_id: String,
    pub asset_symbol: String,
    pub outcome: String,
    pub side: String,
    pub order_type: String,
    pub worst_price: f64,
    pub amount_or_size: f64,
    pub max_notional: f64,
    pub max_slippage_bps: u64,
    pub max_fee_estimate: f64,
    pub max_open_orders_after_run: usize,
    pub retry_count: u64,
    pub market_end_unix: u64,
    pub min_order_size: f64,
    pub tick_size: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveAlphaCurrentPreflight {
    pub run_id: String,
    pub host_id: String,
    pub geoblock_passed: bool,
    pub geoblock_result: String,
    pub wallet_id: String,
    pub funder_id: String,
    pub signature_type: String,
    pub live_alpha_enabled: bool,
    pub live_alpha_mode: String,
    pub fill_canary_enabled: bool,
    pub allow_fak: bool,
    pub allow_fok: bool,
    pub allow_marketable_limit: bool,
    pub compile_time_orders_enabled: bool,
    pub cli_intent_enabled: bool,
    pub human_approved: bool,
    pub kill_switch_active: bool,
    pub account_preflight_passed: bool,
    pub account_preflight_live_network_enabled: bool,
    pub available_pusd_units: u64,
    pub allowance_pusd_units: u64,
    pub reserved_pusd_units: u64,
    pub open_order_count: usize,
    pub recent_trade_count: usize,
    pub heartbeat_status: String,
    pub market_found: bool,
    pub market_active: bool,
    pub market_closed: bool,
    pub market_accepting_orders: bool,
    pub current_market_slug: Option<String>,
    pub current_condition_id: Option<String>,
    pub current_token_id: Option<String>,
    pub current_asset_symbol: Option<String>,
    pub current_outcome: Option<String>,
    pub current_market_end_unix: Option<u64>,
    pub current_min_order_size: Option<f64>,
    pub current_tick_size: Option<f64>,
    pub best_bid: Option<f64>,
    pub best_bid_size: Option<f64>,
    pub best_ask: Option<f64>,
    pub best_ask_size: Option<f64>,
    pub book_snapshot_id: Option<String>,
    pub book_age_ms: Option<u64>,
    pub max_book_age_ms: u64,
    pub reference_snapshot_id: Option<String>,
    pub reference_age_ms: Option<u64>,
    pub max_reference_age_ms: u64,
    pub journal_path_present: bool,
    pub journal_replay_passed: bool,
    pub prior_attempt_consumed: bool,
    pub now_unix: u64,
    pub no_trade_seconds_before_close: u64,
    pub canary_secret_handles_present: bool,
    pub l2_secret_handles_present: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveAlphaPreflightReport {
    pub status: &'static str,
    pub mode: &'static str,
    pub block_reasons: Vec<&'static str>,
    pub run_id: String,
    pub approval_id: String,
    pub host_id: String,
    pub wallet_id: String,
    pub funder_id: String,
    pub geoblock_result: String,
    pub account_preflight_passed: bool,
    pub account_preflight_live_network_enabled: bool,
    pub available_pusd_units: u64,
    pub allowance_pusd_units: u64,
    pub reserved_pusd_units: u64,
    pub open_order_count: usize,
    pub recent_trade_count: usize,
    pub heartbeat_status: String,
    pub market_slug: String,
    pub condition_id: String,
    pub token_id: String,
    pub asset_symbol: String,
    pub outcome: String,
    pub side: String,
    pub order_type: String,
    pub price: f64,
    pub amount_or_size: f64,
    pub max_notional: f64,
    pub max_slippage_bps: u64,
    pub max_fee_estimate: f64,
    pub official_taker_fee_estimate: Option<f64>,
    pub book_snapshot_id: String,
    pub book_age_ms: Option<u64>,
    pub reference_snapshot_id: String,
    pub reference_age_ms: Option<u64>,
    pub compile_time_orders_enabled: bool,
    pub prior_attempt_consumed: bool,
}

impl LiveAlphaPreflightReport {
    pub fn passed(&self) -> bool {
        self.block_reasons.is_empty()
    }
}

pub fn evaluate_live_alpha_preflight(
    mode: LiveAlphaPreflightMode,
    approved: &LiveAlphaApprovedOrderBounds,
    current: &LiveAlphaCurrentPreflight,
) -> LiveAlphaPreflightReport {
    let mut block_reasons = Vec::new();
    let official_taker_fee_estimate = official_crypto_taker_fee_estimate(approved, current);

    validate_identity_and_account(approved, current, &mut block_reasons);
    validate_config(mode, current, &mut block_reasons);
    validate_order_bounds(
        approved,
        current,
        official_taker_fee_estimate,
        &mut block_reasons,
    );
    validate_market_and_book(approved, current, &mut block_reasons);
    validate_journal_and_attempt_cap(mode, current, &mut block_reasons);

    dedupe_preserving_order(&mut block_reasons);

    LiveAlphaPreflightReport {
        status: if block_reasons.is_empty() {
            "passed"
        } else {
            "blocked"
        },
        mode: mode.as_str(),
        block_reasons,
        run_id: current.run_id.clone(),
        approval_id: approved.approval_id.clone(),
        host_id: current.host_id.clone(),
        wallet_id: current.wallet_id.clone(),
        funder_id: current.funder_id.clone(),
        geoblock_result: current.geoblock_result.clone(),
        account_preflight_passed: current.account_preflight_passed,
        account_preflight_live_network_enabled: current.account_preflight_live_network_enabled,
        available_pusd_units: current.available_pusd_units,
        allowance_pusd_units: current.allowance_pusd_units,
        reserved_pusd_units: current.reserved_pusd_units,
        open_order_count: current.open_order_count,
        recent_trade_count: current.recent_trade_count,
        heartbeat_status: current.heartbeat_status.clone(),
        market_slug: approved.market_slug.clone(),
        condition_id: approved.condition_id.clone(),
        token_id: approved.token_id.clone(),
        asset_symbol: approved.asset_symbol.clone(),
        outcome: approved.outcome.clone(),
        side: approved.side.clone(),
        order_type: approved.order_type.clone(),
        price: approved.worst_price,
        amount_or_size: approved.amount_or_size,
        max_notional: approved.max_notional,
        max_slippage_bps: approved.max_slippage_bps,
        max_fee_estimate: approved.max_fee_estimate,
        official_taker_fee_estimate,
        book_snapshot_id: current
            .book_snapshot_id
            .clone()
            .unwrap_or_else(|| "missing".to_string()),
        book_age_ms: current.book_age_ms,
        reference_snapshot_id: current
            .reference_snapshot_id
            .clone()
            .unwrap_or_else(|| "missing".to_string()),
        reference_age_ms: current.reference_age_ms,
        compile_time_orders_enabled: current.compile_time_orders_enabled,
        prior_attempt_consumed: current.prior_attempt_consumed,
    }
}

fn validate_identity_and_account(
    approved: &LiveAlphaApprovedOrderBounds,
    current: &LiveAlphaCurrentPreflight,
    block_reasons: &mut Vec<&'static str>,
) {
    if approved.approval_id.trim().is_empty() {
        block_reasons.push("approval_id_missing");
    }
    if approved.approved_host_ids.is_empty() || !host_matches(approved, &current.host_id) {
        block_reasons.push("approved_host_mismatch");
    }
    if !eq_ignore_ascii(&approved.wallet_id, &current.wallet_id) {
        block_reasons.push("approved_wallet_mismatch");
    }
    if !eq_ignore_ascii(&approved.funder_id, &current.funder_id) {
        block_reasons.push("approved_funder_mismatch");
    }
    if normalize_signature_type(&approved.signature_type)
        != normalize_signature_type(&current.signature_type)
    {
        block_reasons.push("signature_type_mismatch");
    }
    if !current.geoblock_passed {
        block_reasons.push("geoblock_not_passed");
    }
    if !current.account_preflight_passed {
        block_reasons.push("account_preflight_not_passed");
    }
    if !current.account_preflight_live_network_enabled {
        block_reasons.push("account_preflight_not_live_network");
    }
    if current.reserved_pusd_units != 0 {
        block_reasons.push("reserved_pusd_nonzero");
    }
    if current.open_order_count != 0 {
        block_reasons.push("open_orders_nonzero");
    }
    if !matches!(
        current.heartbeat_status.as_str(),
        "not_started_no_open_orders" | "healthy"
    ) {
        block_reasons.push("heartbeat_not_ready");
    }
}

fn validate_config(
    mode: LiveAlphaPreflightMode,
    current: &LiveAlphaCurrentPreflight,
    block_reasons: &mut Vec<&'static str>,
) {
    if !current.live_alpha_enabled {
        block_reasons.push("live_alpha_disabled");
    }
    if current.live_alpha_mode != "fill_canary" {
        block_reasons.push("live_alpha_mode_not_fill_canary");
    }
    if !current.fill_canary_enabled {
        block_reasons.push("fill_canary_disabled");
    }
    if !current.allow_fak {
        block_reasons.push("fak_not_enabled_in_config");
    }
    if current.allow_fok {
        block_reasons.push("fok_enabled_but_not_approved");
    }
    if current.allow_marketable_limit {
        block_reasons.push("marketable_limit_enabled_but_not_approved");
    }
    if current.kill_switch_active {
        block_reasons.push("kill_switch_active");
    }
    if !current.cli_intent_enabled {
        block_reasons.push("missing_cli_intent");
    }
    if mode.requires_submit_gates() && !current.compile_time_orders_enabled {
        block_reasons.push("compile_time_live_disabled");
    }
    if mode.requires_submit_gates() && !current.human_approved {
        block_reasons.push("human_approval_missing");
    }
    if mode.requires_submit_gates() && !current.canary_secret_handles_present {
        block_reasons.push("canary_secret_handles_missing");
    }
    if !current.l2_secret_handles_present {
        block_reasons.push("l2_secret_handles_missing");
    }
}

fn validate_order_bounds(
    approved: &LiveAlphaApprovedOrderBounds,
    current: &LiveAlphaCurrentPreflight,
    official_taker_fee_estimate: Option<f64>,
    block_reasons: &mut Vec<&'static str>,
) {
    if !eq_ignore_ascii(&approved.side, "BUY") {
        block_reasons.push("side_not_approved_buy");
    }
    if !eq_ignore_ascii(&approved.order_type, "FAK") {
        block_reasons.push("order_type_not_approved_fak");
    }
    if approved.retry_count != 0 {
        block_reasons.push("retry_count_nonzero");
    }
    if approved.max_open_orders_after_run != 0 {
        block_reasons.push("max_open_orders_after_run_nonzero");
    }
    if !valid_price(approved.worst_price) {
        block_reasons.push("worst_price_invalid");
    }
    if approved.amount_or_size <= 0.0 || !approved.amount_or_size.is_finite() {
        block_reasons.push("amount_or_size_invalid");
    }
    if approved.max_notional <= 0.0 || approved.amount_or_size > approved.max_notional {
        block_reasons.push("notional_exceeds_approval");
    }
    if approved.max_fee_estimate <= 0.0 || !approved.max_fee_estimate.is_finite() {
        block_reasons.push("max_fee_estimate_invalid");
    }
    if let Some(fee_estimate) = official_taker_fee_estimate {
        if fee_estimate > approved.max_fee_estimate + 1e-9 {
            block_reasons.push("approved_fee_estimate_below_official_taker_fee");
        }
    }
    let required_units = decimal_to_fixed6_units(approved.max_notional + approved.max_fee_estimate);
    if current.available_pusd_units < required_units {
        block_reasons.push("available_pusd_below_notional_plus_fee");
    }
    if current.allowance_pusd_units < required_units {
        block_reasons.push("allowance_below_notional_plus_fee");
    }
    if approved.min_order_size > 0.0
        && valid_price(approved.worst_price)
        && approved.amount_or_size / approved.worst_price < approved.min_order_size
    {
        block_reasons.push("amount_below_market_min_size_at_worst_price");
    }
    if approved.tick_size > 0.0 && !tick_aligned(approved.worst_price, approved.tick_size) {
        block_reasons.push("worst_price_not_tick_aligned");
    }
}

fn official_crypto_taker_fee_estimate(
    approved: &LiveAlphaApprovedOrderBounds,
    current: &LiveAlphaCurrentPreflight,
) -> Option<f64> {
    if !["BTC", "ETH", "SOL"].contains(&approved.asset_symbol.to_ascii_uppercase().as_str()) {
        return None;
    }
    let execution_price = current.best_ask?;
    if approved.amount_or_size <= 0.0 || !approved.amount_or_size.is_finite() {
        return None;
    }
    if !valid_price(execution_price) {
        return None;
    }
    let shares_traded = approved.amount_or_size / execution_price;
    let fee = shares_traded
        * POLYMARKET_CRYPTO_TAKER_FEE_RATE
        * execution_price
        * (1.0 - execution_price);
    Some(round_to_fee_precision(fee))
}

fn validate_market_and_book(
    approved: &LiveAlphaApprovedOrderBounds,
    current: &LiveAlphaCurrentPreflight,
    block_reasons: &mut Vec<&'static str>,
) {
    if !current.market_found {
        block_reasons.push("market_not_found_or_closed");
    }
    if !current.market_active {
        block_reasons.push("market_not_active");
    }
    if current.market_closed {
        block_reasons.push("market_closed");
    }
    if !current.market_accepting_orders {
        block_reasons.push("market_not_accepting_orders");
    }
    if !option_eq_ignore_ascii(
        current.current_market_slug.as_deref(),
        &approved.market_slug,
    ) {
        block_reasons.push("market_slug_mismatch");
    }
    if !option_eq_ignore_ascii(
        current.current_condition_id.as_deref(),
        &approved.condition_id,
    ) {
        block_reasons.push("condition_id_mismatch");
    }
    if !option_eq_ignore_ascii(current.current_token_id.as_deref(), &approved.token_id) {
        block_reasons.push("token_id_mismatch");
    }
    if !option_eq_ignore_ascii(
        current.current_asset_symbol.as_deref(),
        &approved.asset_symbol,
    ) {
        block_reasons.push("asset_mismatch");
    }
    if !option_eq_ignore_ascii(current.current_outcome.as_deref(), &approved.outcome) {
        block_reasons.push("outcome_mismatch");
    }
    if current.current_market_end_unix != Some(approved.market_end_unix) {
        block_reasons.push("market_end_mismatch");
    }
    if let Some(current_min) = current.current_min_order_size {
        if (current_min - approved.min_order_size).abs() > 1e-9 {
            block_reasons.push("market_min_order_size_mismatch");
        }
    }
    if let Some(current_tick) = current.current_tick_size {
        if (current_tick - approved.tick_size).abs() > 1e-9 {
            block_reasons.push("market_tick_size_mismatch");
        }
    }
    if current.now_unix
        >= approved
            .market_end_unix
            .saturating_sub(current.no_trade_seconds_before_close)
    {
        block_reasons.push("market_expired_or_past_no_trade_cutoff");
    }
    match current.book_age_ms {
        Some(age) if age <= current.max_book_age_ms => {}
        Some(_) => block_reasons.push("book_stale"),
        None => block_reasons.push("book_missing"),
    }
    match current.reference_age_ms {
        Some(age) if age <= current.max_reference_age_ms => {}
        Some(_) => block_reasons.push("reference_stale"),
        None => block_reasons.push("reference_missing"),
    }
    let Some(best_ask) = current.best_ask else {
        block_reasons.push("best_ask_missing");
        return;
    };
    if best_ask > approved.worst_price {
        block_reasons.push("best_ask_exceeds_worst_price");
    }
    if best_ask > 0.0 {
        let slippage_bps = ((approved.worst_price - best_ask).max(0.0) / best_ask) * 10_000.0;
        if slippage_bps > approved.max_slippage_bps as f64 + 1e-9 {
            block_reasons.push("slippage_exceeds_approval");
        }
    }
    if let Some(best_ask_size) = current.best_ask_size {
        if best_ask_size * best_ask < approved.amount_or_size {
            block_reasons.push("visible_best_ask_notional_below_amount");
        }
    } else {
        block_reasons.push("best_ask_size_missing");
    }
}

fn validate_journal_and_attempt_cap(
    mode: LiveAlphaPreflightMode,
    current: &LiveAlphaCurrentPreflight,
    block_reasons: &mut Vec<&'static str>,
) {
    if !current.journal_path_present {
        block_reasons.push("journal_path_missing");
    }
    if !current.journal_replay_passed {
        block_reasons.push("journal_replay_not_healthy");
    }
    if current.prior_attempt_consumed {
        block_reasons.push("approval_attempt_already_consumed");
    }
    if mode.requires_submit_gates() && current.prior_attempt_consumed {
        block_reasons.push("second_attempt_refused");
    }
}

fn host_matches(approved: &LiveAlphaApprovedOrderBounds, current_host: &str) -> bool {
    approved.approved_host_ids.iter().any(|approved_host| {
        eq_ignore_ascii(approved_host, current_host)
            || current_host
                .to_ascii_lowercase()
                .contains(&approved_host.to_ascii_lowercase())
            || approved_host
                .to_ascii_lowercase()
                .contains(&current_host.to_ascii_lowercase())
    })
}

fn eq_ignore_ascii(lhs: &str, rhs: &str) -> bool {
    lhs.trim().eq_ignore_ascii_case(rhs.trim())
}

fn option_eq_ignore_ascii(lhs: Option<&str>, rhs: &str) -> bool {
    lhs.is_some_and(|lhs| eq_ignore_ascii(lhs, rhs))
}

fn normalize_signature_type(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace(['-', ' '], "_")
}

fn valid_price(value: f64) -> bool {
    value.is_finite() && value > 0.0 && value < 1.0
}

fn tick_aligned(price: f64, tick_size: f64) -> bool {
    let ticks = price / tick_size;
    (ticks - ticks.round()).abs() < 1e-9
}

fn decimal_to_fixed6_units(value: f64) -> u64 {
    (value * 1_000_000.0).ceil().max(0.0) as u64
}

fn round_to_fee_precision(value: f64) -> f64 {
    (value * 100_000.0).round() / 100_000.0
}

fn dedupe_preserving_order(values: &mut Vec<&'static str>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(*value));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_alpha_preflight_passes_exact_fak_bounds() {
        let approved = approved_bounds();
        let current = current_preflight();
        let report =
            evaluate_live_alpha_preflight(LiveAlphaPreflightMode::FinalSubmit, &approved, &current);

        assert_eq!(report.status, "passed");
        assert!(report.passed());
    }

    #[test]
    fn live_alpha_preflight_blocks_identity_and_artifact_mismatch() {
        let approved = approved_bounds();
        let mut current = current_preflight();
        current.host_id = "other-host".to_string();
        current.wallet_id = "0x1111111111111111111111111111111111111111".to_string();
        current.current_token_id = Some("999".to_string());

        let report =
            evaluate_live_alpha_preflight(LiveAlphaPreflightMode::FinalSubmit, &approved, &current);

        assert_eq!(report.status, "blocked");
        assert!(report.block_reasons.contains(&"approved_host_mismatch"));
        assert!(report.block_reasons.contains(&"approved_wallet_mismatch"));
        assert!(report.block_reasons.contains(&"token_id_mismatch"));
    }

    #[test]
    fn live_alpha_preflight_blocks_stale_or_closed_market() {
        let approved = approved_bounds();
        let mut current = current_preflight();
        current.market_active = false;
        current.market_closed = true;
        current.now_unix = approved.market_end_unix;

        let report =
            evaluate_live_alpha_preflight(LiveAlphaPreflightMode::FinalSubmit, &approved, &current);

        assert!(report.block_reasons.contains(&"market_not_active"));
        assert!(report.block_reasons.contains(&"market_closed"));
        assert!(report
            .block_reasons
            .contains(&"market_expired_or_past_no_trade_cutoff"));
    }

    #[test]
    fn live_alpha_preflight_blocks_min_size_and_attempt_cap() {
        let mut approved = approved_bounds();
        approved.amount_or_size = 1.0;
        approved.worst_price = 0.55;
        approved.min_order_size = 5.0;
        let mut current = current_preflight();
        current.prior_attempt_consumed = true;

        let report =
            evaluate_live_alpha_preflight(LiveAlphaPreflightMode::FinalSubmit, &approved, &current);

        assert!(report
            .block_reasons
            .contains(&"amount_below_market_min_size_at_worst_price"));
        assert!(report
            .block_reasons
            .contains(&"approval_attempt_already_consumed"));
        assert!(report.block_reasons.contains(&"second_attempt_refused"));
    }

    #[test]
    fn live_alpha_preflight_blocks_underestimated_official_taker_fee() {
        let mut approved = approved_bounds();
        approved.amount_or_size = 2.56;
        approved.max_notional = 2.56;
        approved.worst_price = 0.51;
        approved.max_fee_estimate = 0.06;
        let mut current = current_preflight();
        current.best_ask = Some(0.50);
        current.best_ask_size = Some(10.0);

        let report =
            evaluate_live_alpha_preflight(LiveAlphaPreflightMode::FinalSubmit, &approved, &current);

        assert_eq!(report.official_taker_fee_estimate, Some(0.09216));
        assert!(report
            .block_reasons
            .contains(&"approved_fee_estimate_below_official_taker_fee"));
    }

    fn approved_bounds() -> LiveAlphaApprovedOrderBounds {
        LiveAlphaApprovedOrderBounds {
            approval_id: "LA3-2026-05-04-001".to_string(),
            approved_host_ids: vec!["approved-host".to_string()],
            wallet_id: "0x280ca8b14386Fe4203670538CCdE636C295d74E9".to_string(),
            funder_id: "0xB06867f742290D25B7430fD35D7A8cE7bc3a1159".to_string(),
            signature_type: "poly_proxy".to_string(),
            market_slug: "btc-updown-15m-1777909500".to_string(),
            condition_id: "0x371c52ca5f8dbe256978e6d27f6a6d8cf64f3722b15e44ba3128685ccfbeee0c"
                .to_string(),
            token_id:
                "91899612655270438973839203540142703788805338252926995927363610489118446263952"
                    .to_string(),
            asset_symbol: "BTC".to_string(),
            outcome: "Up".to_string(),
            side: "BUY".to_string(),
            order_type: "FAK".to_string(),
            worst_price: 0.55,
            amount_or_size: 5.5,
            max_notional: 5.5,
            max_slippage_bps: 1_000,
            max_fee_estimate: 0.20,
            max_open_orders_after_run: 0,
            retry_count: 0,
            market_end_unix: 1_777_909_600,
            min_order_size: 5.0,
            tick_size: 0.01,
        }
    }

    fn current_preflight() -> LiveAlphaCurrentPreflight {
        LiveAlphaCurrentPreflight {
            run_id: "run-1".to_string(),
            host_id: "approved-host".to_string(),
            geoblock_passed: true,
            geoblock_result: "status=passed,country=MX,region=CHP".to_string(),
            wallet_id: "0x280ca8b14386Fe4203670538CCdE636C295d74E9".to_string(),
            funder_id: "0xB06867f742290D25B7430fD35D7A8cE7bc3a1159".to_string(),
            signature_type: "poly_proxy".to_string(),
            live_alpha_enabled: true,
            live_alpha_mode: "fill_canary".to_string(),
            fill_canary_enabled: true,
            allow_fak: true,
            allow_fok: false,
            allow_marketable_limit: false,
            compile_time_orders_enabled: true,
            cli_intent_enabled: true,
            human_approved: true,
            kill_switch_active: false,
            account_preflight_passed: true,
            account_preflight_live_network_enabled: true,
            available_pusd_units: 10_000_000,
            allowance_pusd_units: 10_000_000,
            reserved_pusd_units: 0,
            open_order_count: 0,
            recent_trade_count: 0,
            heartbeat_status: "not_started_no_open_orders".to_string(),
            market_found: true,
            market_active: true,
            market_closed: false,
            market_accepting_orders: true,
            current_market_slug: Some("btc-updown-15m-1777909500".to_string()),
            current_condition_id: Some(
                "0x371c52ca5f8dbe256978e6d27f6a6d8cf64f3722b15e44ba3128685ccfbeee0c".to_string(),
            ),
            current_token_id: Some(
                "91899612655270438973839203540142703788805338252926995927363610489118446263952"
                    .to_string(),
            ),
            current_asset_symbol: Some("BTC".to_string()),
            current_outcome: Some("Up".to_string()),
            current_market_end_unix: Some(1_777_909_600),
            current_min_order_size: Some(5.0),
            current_tick_size: Some(0.01),
            best_bid: Some(0.49),
            best_bid_size: Some(65.0),
            best_ask: Some(0.50),
            best_ask_size: Some(75.0),
            book_snapshot_id: Some("book-hash".to_string()),
            book_age_ms: Some(100),
            max_book_age_ms: 5_000,
            reference_snapshot_id: Some("reference".to_string()),
            reference_age_ms: Some(100),
            max_reference_age_ms: 5_000,
            journal_path_present: true,
            journal_replay_passed: true,
            prior_attempt_consumed: false,
            now_unix: 1_777_909_000,
            no_trade_seconds_before_close: 30,
            canary_secret_handles_present: true,
            l2_secret_handles_present: true,
        }
    }
}
