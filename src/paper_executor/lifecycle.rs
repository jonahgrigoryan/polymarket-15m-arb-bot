use std::collections::BTreeMap;
use std::fmt;

use crate::domain::{
    FeeParameters, OrderKind, PaperFill, PaperOrder, PaperOrderIntent, PaperOrderStatus, Side,
};
use crate::events::NormalizedEvent;
use crate::risk_engine::RiskGateDecision;
use crate::state::{LastTradeState, PriceLevelSnapshot, TokenBookSnapshot};

const PRICE_EPSILON: f64 = 1e-9;
const SIZE_EPSILON: f64 = 1e-9;

#[derive(Debug, Clone, PartialEq)]
pub struct PaperExecutorConfig {
    pub order_id_prefix: String,
    pub fill_id_prefix: String,
}

impl Default for PaperExecutorConfig {
    fn default() -> Self {
        Self {
            order_id_prefix: "paper-order".to_string(),
            fill_id_prefix: "paper-fill".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FillSimulationInput {
    pub order_id: String,
    pub book: TokenBookSnapshot,
    pub last_trade: Option<LastTradeState>,
    pub now_ts: i64,
}

impl FillSimulationInput {
    pub fn new(order_id: impl Into<String>, book: TokenBookSnapshot, now_ts: i64) -> Self {
        let last_trade = book.last_trade.clone();
        Self {
            order_id: order_id.into(),
            book,
            last_trade,
            now_ts,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaperExecutionAuditEvent {
    RiskRejected {
        market_id: String,
        token_id: String,
        reason: String,
    },
    OrderCreated {
        order_id: String,
        status: PaperOrderStatus,
        created_ts: i64,
    },
    OrderOpened {
        order_id: String,
        opened_ts: i64,
    },
    OrderRejected {
        order_id: String,
        reason: String,
        rejected_ts: i64,
    },
    OrderCanceled {
        order_id: String,
        reason: String,
        canceled_ts: i64,
    },
    OrderExpired {
        order_id: String,
        reason: String,
        expired_ts: i64,
    },
    FillSimulated {
        order_id: String,
        fill_id: String,
        price: f64,
        size: f64,
        liquidity: OrderKind,
        queue_ahead_remaining: Option<f64>,
        filled_ts: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PaperExecutionResult {
    pub order: Option<PaperOrder>,
    pub fills: Vec<PaperFill>,
    pub audit_events: Vec<PaperExecutionAuditEvent>,
    pub normalized_events: Vec<NormalizedEvent>,
}

impl PaperExecutionResult {
    fn extend(&mut self, other: PaperExecutionResult) {
        if other.order.is_some() {
            self.order = other.order;
        }
        self.fills.extend(other.fills);
        self.audit_events.extend(other.audit_events);
        self.normalized_events.extend(other.normalized_events);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaperExecutionError {
    OrderNotFound(String),
    BookMismatch {
        order_id: String,
        order_market_id: String,
        order_token_id: String,
        book_market_id: String,
        book_token_id: String,
    },
    TerminalOrder {
        order_id: String,
        status: PaperOrderStatus,
    },
    MakerQueueMissing(String),
}

impl fmt::Display for PaperExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PaperExecutionError::OrderNotFound(order_id) => {
                write!(f, "paper order {order_id} was not found")
            }
            PaperExecutionError::BookMismatch {
                order_id,
                order_market_id,
                order_token_id,
                book_market_id,
                book_token_id,
            } => write!(
                f,
                "book {book_market_id}/{book_token_id} does not match paper order {order_id} {order_market_id}/{order_token_id}"
            ),
            PaperExecutionError::TerminalOrder { order_id, status } => {
                write!(f, "paper order {order_id} is terminal: {status:?}")
            }
            PaperExecutionError::MakerQueueMissing(order_id) => {
                write!(f, "maker queue state missing for paper order {order_id}")
            }
        }
    }
}

impl std::error::Error for PaperExecutionError {}

#[derive(Debug, Clone, PartialEq)]
pub struct PaperExecutor {
    config: PaperExecutorConfig,
    orders: BTreeMap<String, PaperOrder>,
    maker_queues: BTreeMap<String, MakerQueueState>,
    next_order_seq: u64,
    next_fill_seq: u64,
}

impl PaperExecutor {
    pub fn new(config: PaperExecutorConfig) -> Self {
        Self {
            config,
            orders: BTreeMap::new(),
            maker_queues: BTreeMap::new(),
            next_order_seq: 1,
            next_fill_seq: 1,
        }
    }

    pub fn order(&self, order_id: &str) -> Option<&PaperOrder> {
        self.orders.get(order_id)
    }

    pub fn orders(&self) -> Vec<&PaperOrder> {
        self.orders.values().collect()
    }

    pub fn open_paper_order(
        &mut self,
        intent: PaperOrderIntent,
        risk_decision: &RiskGateDecision,
        fee_parameters: &FeeParameters,
        book: Option<&TokenBookSnapshot>,
        now_ts: i64,
    ) -> Result<PaperExecutionResult, PaperExecutionError> {
        if !risk_decision.approved {
            return Ok(self.risk_rejected_result(&intent, risk_decision));
        }

        if let Some(reason) = invalid_intent_reason(&intent) {
            return Ok(self.reject_approved_intent(intent, fee_parameters, reason, now_ts));
        }

        let book = match book {
            Some(book) if book_matches_intent(book, &intent) => book,
            Some(book) => {
                let reason = format!(
                    "book {} / {} does not match intent {} / {}",
                    book.market_id, book.token_id, intent.market_id, intent.token_id
                );
                return Ok(self.reject_approved_intent(intent, fee_parameters, reason, now_ts));
            }
            None => {
                return Ok(self.reject_approved_intent(
                    intent,
                    fee_parameters,
                    "book snapshot is required for paper fill simulation".to_string(),
                    now_ts,
                ));
            }
        };

        let order = self.build_paper_order(
            intent,
            fee_parameters.clone(),
            PaperOrderStatus::Open,
            now_ts,
        );
        let order_id = order.order_id.clone();
        if order.order_kind == OrderKind::Maker {
            self.maker_queues.insert(
                order_id.clone(),
                MakerQueueState {
                    queue_ahead_remaining: visible_size_at_limit(book, order.side, order.price),
                    last_seen_trade: book.last_trade.as_ref().map(last_trade_key),
                },
            );
        }

        let mut result = PaperExecutionResult {
            order: Some(order.clone()),
            fills: Vec::new(),
            audit_events: vec![
                PaperExecutionAuditEvent::OrderCreated {
                    order_id: order_id.clone(),
                    status: PaperOrderStatus::Created,
                    created_ts: now_ts,
                },
                PaperExecutionAuditEvent::OrderOpened {
                    order_id: order_id.clone(),
                    opened_ts: now_ts,
                },
            ],
            normalized_events: vec![NormalizedEvent::PaperOrderPlaced { order }],
        };

        if self
            .order(&order_id)
            .map(|order| order.order_kind == OrderKind::Taker)
            .unwrap_or(false)
        {
            result.extend(self.simulate_taker_fill(&order_id, book, now_ts)?);
        }

        Ok(result)
    }

    pub fn simulate_fill(
        &mut self,
        input: FillSimulationInput,
    ) -> Result<PaperExecutionResult, PaperExecutionError> {
        let order = self
            .orders
            .get(&input.order_id)
            .ok_or_else(|| PaperExecutionError::OrderNotFound(input.order_id.clone()))?;

        ensure_order_can_fill(order)?;
        ensure_book_matches_order(order, &input.book)?;

        match order.order_kind {
            OrderKind::Maker => self.simulate_maker_fill(input),
            OrderKind::Taker => {
                self.simulate_taker_fill(&input.order_id, &input.book, input.now_ts)
            }
        }
    }

    pub fn cancel_order(
        &mut self,
        order_id: &str,
        reason: impl Into<String>,
        now_ts: i64,
    ) -> Result<PaperExecutionResult, PaperExecutionError> {
        let reason = reason.into();
        let order = self
            .orders
            .get_mut(order_id)
            .ok_or_else(|| PaperExecutionError::OrderNotFound(order_id.to_string()))?;

        ensure_order_can_close(order)?;

        order.status = PaperOrderStatus::Canceled;
        order.updated_ts = now_ts;
        let order = order.clone();
        self.maker_queues.remove(order_id);

        Ok(PaperExecutionResult {
            order: Some(order.clone()),
            fills: Vec::new(),
            audit_events: vec![PaperExecutionAuditEvent::OrderCanceled {
                order_id: order.order_id.clone(),
                reason: reason.clone(),
                canceled_ts: now_ts,
            }],
            normalized_events: vec![NormalizedEvent::PaperOrderCanceled {
                order_id: order.order_id,
                market_id: order.market_id,
                reason,
                canceled_ts: now_ts,
            }],
        })
    }

    pub fn expire_order(
        &mut self,
        order_id: &str,
        reason: impl Into<String>,
        now_ts: i64,
    ) -> Result<PaperExecutionResult, PaperExecutionError> {
        let reason = reason.into();
        let order = self
            .orders
            .get_mut(order_id)
            .ok_or_else(|| PaperExecutionError::OrderNotFound(order_id.to_string()))?;

        ensure_order_can_close(order)?;

        order.status = PaperOrderStatus::Expired;
        order.updated_ts = now_ts;
        let order = order.clone();
        self.maker_queues.remove(order_id);

        Ok(PaperExecutionResult {
            order: Some(order.clone()),
            fills: Vec::new(),
            audit_events: vec![PaperExecutionAuditEvent::OrderExpired {
                order_id: order.order_id,
                reason,
                expired_ts: now_ts,
            }],
            normalized_events: Vec::new(),
        })
    }

    pub fn reject_intent(
        &mut self,
        intent: PaperOrderIntent,
        risk_decision: &RiskGateDecision,
        fee_parameters: &FeeParameters,
        reason: impl Into<String>,
        now_ts: i64,
    ) -> Result<PaperExecutionResult, PaperExecutionError> {
        if !risk_decision.approved {
            return Ok(self.risk_rejected_result(&intent, risk_decision));
        }

        Ok(self.reject_approved_intent(intent, fee_parameters, reason.into(), now_ts))
    }

    fn risk_rejected_result(
        &self,
        intent: &PaperOrderIntent,
        risk_decision: &RiskGateDecision,
    ) -> PaperExecutionResult {
        PaperExecutionResult {
            order: None,
            fills: Vec::new(),
            audit_events: vec![PaperExecutionAuditEvent::RiskRejected {
                market_id: intent.market_id.clone(),
                token_id: intent.token_id.clone(),
                reason: risk_rejection_reason(risk_decision),
            }],
            normalized_events: Vec::new(),
        }
    }

    fn reject_approved_intent(
        &mut self,
        intent: PaperOrderIntent,
        fee_parameters: &FeeParameters,
        reason: String,
        now_ts: i64,
    ) -> PaperExecutionResult {
        let order = self.build_paper_order(
            intent,
            fee_parameters.clone(),
            PaperOrderStatus::Rejected,
            now_ts,
        );

        PaperExecutionResult {
            order: Some(order.clone()),
            fills: Vec::new(),
            audit_events: vec![
                PaperExecutionAuditEvent::OrderCreated {
                    order_id: order.order_id.clone(),
                    status: PaperOrderStatus::Created,
                    created_ts: now_ts,
                },
                PaperExecutionAuditEvent::OrderRejected {
                    order_id: order.order_id,
                    reason,
                    rejected_ts: now_ts,
                },
            ],
            normalized_events: Vec::new(),
        }
    }

    fn build_paper_order(
        &mut self,
        intent: PaperOrderIntent,
        fee_parameters: FeeParameters,
        status: PaperOrderStatus,
        now_ts: i64,
    ) -> PaperOrder {
        let order = PaperOrder {
            order_id: self.next_order_id(),
            market_id: intent.market_id,
            token_id: intent.token_id,
            asset: intent.asset,
            side: intent.side,
            order_kind: intent.order_kind,
            fee_parameters,
            price: intent.price,
            size: intent.size,
            filled_size: 0.0,
            status,
            reason: intent.reason,
            created_ts: now_ts,
            updated_ts: now_ts,
        };

        self.orders.insert(order.order_id.clone(), order.clone());
        order
    }

    fn simulate_maker_fill(
        &mut self,
        input: FillSimulationInput,
    ) -> Result<PaperExecutionResult, PaperExecutionError> {
        let trade = match input
            .last_trade
            .clone()
            .or_else(|| input.book.last_trade.clone())
        {
            Some(trade) => trade,
            None => return Ok(PaperExecutionResult::default()),
        };

        let (order_price, order_remaining) = {
            let order = self
                .orders
                .get(&input.order_id)
                .ok_or_else(|| PaperExecutionError::OrderNotFound(input.order_id.clone()))?;
            (order.price, remaining_size(order))
        };

        if order_remaining <= SIZE_EPSILON {
            return Ok(PaperExecutionResult::default());
        }

        let (fill_size, queue_ahead_remaining) = {
            let queue = self
                .maker_queues
                .get_mut(&input.order_id)
                .ok_or_else(|| PaperExecutionError::MakerQueueMissing(input.order_id.clone()))?;
            let key = last_trade_key(&trade);

            if queue.last_seen_trade.as_ref() == Some(&key) {
                return Ok(PaperExecutionResult::default());
            }
            queue.last_seen_trade = Some(key);

            if !same_price(trade.price, order_price) || !valid_positive_size(trade.size) {
                return Ok(PaperExecutionResult::default());
            }

            let queue_consumed = queue.queue_ahead_remaining.min(trade.size);
            queue.queue_ahead_remaining -= queue_consumed;
            let fillable_after_queue = trade.size - queue_consumed;

            (
                fillable_after_queue.min(order_remaining),
                Some(queue.queue_ahead_remaining),
            )
        };

        if fill_size <= SIZE_EPSILON {
            return Ok(PaperExecutionResult::default());
        }

        self.apply_fill(
            &input.order_id,
            order_price,
            fill_size,
            OrderKind::Maker,
            input.now_ts,
            queue_ahead_remaining,
        )
    }

    fn simulate_taker_fill(
        &mut self,
        order_id: &str,
        book: &TokenBookSnapshot,
        now_ts: i64,
    ) -> Result<PaperExecutionResult, PaperExecutionError> {
        let order = self
            .orders
            .get(order_id)
            .ok_or_else(|| PaperExecutionError::OrderNotFound(order_id.to_string()))?
            .clone();

        ensure_order_can_fill(&order)?;
        ensure_book_matches_order(&order, book)?;

        let mut levels = executable_taker_levels(book, order.side, order.price);
        let mut result = PaperExecutionResult::default();

        for level in levels.drain(..) {
            let remaining = self
                .order(order_id)
                .map(remaining_size)
                .ok_or_else(|| PaperExecutionError::OrderNotFound(order_id.to_string()))?;
            if remaining <= SIZE_EPSILON {
                break;
            }

            let fill_size = remaining.min(level.size);
            if fill_size <= SIZE_EPSILON {
                continue;
            }

            result.extend(self.apply_fill(
                order_id,
                level.price,
                fill_size,
                OrderKind::Taker,
                now_ts,
                None,
            )?);
        }

        Ok(result)
    }

    fn apply_fill(
        &mut self,
        order_id: &str,
        price: f64,
        requested_size: f64,
        liquidity: OrderKind,
        filled_ts: i64,
        queue_ahead_remaining: Option<f64>,
    ) -> Result<PaperExecutionResult, PaperExecutionError> {
        let order = self
            .orders
            .get(order_id)
            .ok_or_else(|| PaperExecutionError::OrderNotFound(order_id.to_string()))?
            .clone();

        ensure_order_can_fill(&order)?;

        let fill_size = remaining_size(&order).min(requested_size);
        if fill_size <= SIZE_EPSILON {
            return Ok(PaperExecutionResult::default());
        }

        let fill = PaperFill {
            fill_id: self.next_fill_id(),
            order_id: order.order_id.clone(),
            market_id: order.market_id.clone(),
            token_id: order.token_id.clone(),
            asset: order.asset,
            side: order.side,
            price,
            size: fill_size,
            fee_paid: crate::paper_executor::pnl::fee_paid(
                fill_size,
                price,
                liquidity,
                &order.fee_parameters,
            ),
            liquidity,
            filled_ts,
        };

        let updated_order = self
            .orders
            .get_mut(order_id)
            .ok_or_else(|| PaperExecutionError::OrderNotFound(order_id.to_string()))?;
        updated_order.filled_size = (updated_order.filled_size + fill_size).min(updated_order.size);
        updated_order.status = if remaining_size(updated_order) <= SIZE_EPSILON {
            updated_order.filled_size = updated_order.size;
            PaperOrderStatus::Filled
        } else {
            PaperOrderStatus::PartiallyFilled
        };
        updated_order.updated_ts = filled_ts;
        let updated_order = updated_order.clone();

        if updated_order.status == PaperOrderStatus::Filled {
            self.maker_queues.remove(order_id);
        }

        Ok(PaperExecutionResult {
            order: Some(updated_order),
            fills: vec![fill.clone()],
            audit_events: vec![PaperExecutionAuditEvent::FillSimulated {
                order_id: order_id.to_string(),
                fill_id: fill.fill_id.clone(),
                price,
                size: fill_size,
                liquidity,
                queue_ahead_remaining,
                filled_ts,
            }],
            normalized_events: vec![NormalizedEvent::PaperFill { fill }],
        })
    }

    fn next_order_id(&mut self) -> String {
        let order_id = format!("{}-{}", self.config.order_id_prefix, self.next_order_seq);
        self.next_order_seq += 1;
        order_id
    }

    fn next_fill_id(&mut self) -> String {
        let fill_id = format!("{}-{}", self.config.fill_id_prefix, self.next_fill_seq);
        self.next_fill_seq += 1;
        fill_id
    }
}

impl Default for PaperExecutor {
    fn default() -> Self {
        Self::new(PaperExecutorConfig::default())
    }
}

#[derive(Debug, Clone, PartialEq)]
struct MakerQueueState {
    queue_ahead_remaining: f64,
    last_seen_trade: Option<LastTradeKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LastTradeKey {
    side: Side,
    price_bits: u64,
    size_bits: u64,
    source_ts: Option<i64>,
    recv_wall_ts: i64,
}

fn invalid_intent_reason(intent: &PaperOrderIntent) -> Option<String> {
    if !intent.price.is_finite() || intent.price < 0.0 || intent.price > 1.0 {
        return Some(format!("invalid paper order price {}", intent.price));
    }
    if !valid_positive_size(intent.size) {
        return Some(format!("invalid paper order size {}", intent.size));
    }
    if !intent.notional.is_finite() || intent.notional < 0.0 {
        return Some(format!("invalid paper order notional {}", intent.notional));
    }
    None
}

fn risk_rejection_reason(risk_decision: &RiskGateDecision) -> String {
    if let Some(reason) = risk_decision.risk_state.reason.as_ref() {
        return reason.clone();
    }

    let reasons = risk_decision
        .violations
        .iter()
        .map(|violation| violation.message.as_str())
        .collect::<Vec<_>>();

    if reasons.is_empty() {
        "risk gate did not approve paper order intent".to_string()
    } else {
        reasons.join("; ")
    }
}

fn ensure_order_can_fill(order: &PaperOrder) -> Result<(), PaperExecutionError> {
    if terminal_status(order.status) {
        return Err(PaperExecutionError::TerminalOrder {
            order_id: order.order_id.clone(),
            status: order.status,
        });
    }
    Ok(())
}

fn ensure_order_can_close(order: &PaperOrder) -> Result<(), PaperExecutionError> {
    if terminal_status(order.status) {
        return Err(PaperExecutionError::TerminalOrder {
            order_id: order.order_id.clone(),
            status: order.status,
        });
    }
    Ok(())
}

fn terminal_status(status: PaperOrderStatus) -> bool {
    matches!(
        status,
        PaperOrderStatus::Filled
            | PaperOrderStatus::Canceled
            | PaperOrderStatus::Expired
            | PaperOrderStatus::Rejected
    )
}

fn ensure_book_matches_order(
    order: &PaperOrder,
    book: &TokenBookSnapshot,
) -> Result<(), PaperExecutionError> {
    if book.market_id == order.market_id && book.token_id == order.token_id {
        return Ok(());
    }

    Err(PaperExecutionError::BookMismatch {
        order_id: order.order_id.clone(),
        order_market_id: order.market_id.clone(),
        order_token_id: order.token_id.clone(),
        book_market_id: book.market_id.clone(),
        book_token_id: book.token_id.clone(),
    })
}

fn book_matches_intent(book: &TokenBookSnapshot, intent: &PaperOrderIntent) -> bool {
    book.market_id == intent.market_id && book.token_id == intent.token_id
}

fn executable_taker_levels(
    book: &TokenBookSnapshot,
    side: Side,
    limit_price: f64,
) -> Vec<PriceLevelSnapshot> {
    let mut levels = match side {
        Side::Buy => book
            .asks
            .levels
            .iter()
            .filter(|level| level.price <= limit_price + PRICE_EPSILON)
            .cloned()
            .collect::<Vec<_>>(),
        Side::Sell => book
            .bids
            .levels
            .iter()
            .filter(|level| level.price + PRICE_EPSILON >= limit_price)
            .cloned()
            .collect::<Vec<_>>(),
    };

    levels.retain(|level| level.price.is_finite() && valid_positive_size(level.size));
    match side {
        Side::Buy => levels.sort_by(|left, right| left.price.total_cmp(&right.price)),
        Side::Sell => levels.sort_by(|left, right| right.price.total_cmp(&left.price)),
    }
    levels
}

fn visible_size_at_limit(book: &TokenBookSnapshot, side: Side, limit_price: f64) -> f64 {
    let levels = match side {
        Side::Buy => &book.bids.levels,
        Side::Sell => &book.asks.levels,
    };

    levels
        .iter()
        .filter(|level| same_price(level.price, limit_price) && valid_positive_size(level.size))
        .map(|level| level.size)
        .sum()
}

fn remaining_size(order: &PaperOrder) -> f64 {
    (order.size - order.filled_size).max(0.0)
}

fn valid_positive_size(size: f64) -> bool {
    size.is_finite() && size > SIZE_EPSILON
}

fn same_price(left: f64, right: f64) -> bool {
    (left - right).abs() <= PRICE_EPSILON
}

fn last_trade_key(trade: &LastTradeState) -> LastTradeKey {
    LastTradeKey {
        side: trade.side,
        price_bits: trade.price.to_bits(),
        size_bits: trade.size.to_bits(),
        source_ts: trade.source_ts,
        recv_wall_ts: trade.recv_wall_ts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Asset, RiskHaltReason, RiskState};
    use crate::risk_engine::RiskViolation;
    use crate::state::BookSideSnapshot;

    const NOW: i64 = 1_777_000_000_000;

    #[test]
    fn maker_fill_waits_until_visible_queue_is_consumed() {
        let mut executor = PaperExecutor::default();
        let book = book(vec![level(0.49, 100.0)], vec![level(0.51, 80.0)], None);
        let placed = executor
            .open_paper_order(
                intent(OrderKind::Maker, Side::Buy, 0.49, 10.0),
                &approved(),
                &fee_parameters(),
                Some(&book),
                NOW,
            )
            .expect("order places");
        let order_id = placed.order.expect("placed order").order_id;

        let queue_only = executor
            .simulate_fill(FillSimulationInput {
                order_id: order_id.clone(),
                book: book_with_trade(Side::Sell, 0.49, 100.0, NOW + 1),
                last_trade: None,
                now_ts: NOW + 1,
            })
            .expect("queue trade processes");

        assert!(queue_only.fills.is_empty());
        assert_eq!(executor.order(&order_id).expect("order").filled_size, 0.0);
        assert_eq!(
            executor.order(&order_id).expect("order").status,
            PaperOrderStatus::Open
        );

        let partial = executor
            .simulate_fill(FillSimulationInput {
                order_id: order_id.clone(),
                book: book_with_trade(Side::Sell, 0.49, 4.0, NOW + 2),
                last_trade: None,
                now_ts: NOW + 2,
            })
            .expect("partial fill processes");

        assert_eq!(partial.fills[0].size, 4.0);
        assert_eq!(
            partial.order.expect("partial order").status,
            PaperOrderStatus::PartiallyFilled
        );
        assert_eq!(
            partial.normalized_events[0].event_type(),
            crate::events::EventType::PaperFill
        );

        let filled = executor
            .simulate_fill(FillSimulationInput {
                order_id: order_id.clone(),
                book: book_with_trade(Side::Sell, 0.49, 10.0, NOW + 3),
                last_trade: None,
                now_ts: NOW + 3,
            })
            .expect("final fill processes");

        assert_eq!(filled.fills[0].size, 6.0);
        assert_eq!(
            executor.order(&order_id).expect("order").status,
            PaperOrderStatus::Filled
        );
    }

    #[test]
    fn taker_buy_consumes_visible_ask_depth() {
        let mut executor = PaperExecutor::default();
        let book = book(
            vec![level(0.48, 100.0)],
            vec![level(0.50, 4.0), level(0.51, 6.0), level(0.53, 100.0)],
            None,
        );

        let result = executor
            .open_paper_order(
                intent(OrderKind::Taker, Side::Buy, 0.52, 10.0),
                &approved(),
                &fee_parameters(),
                Some(&book),
                NOW,
            )
            .expect("taker order places");

        assert_eq!(result.fills.len(), 2);
        assert_eq!(result.fills[0].price, 0.50);
        assert_eq!(result.fills[0].size, 4.0);
        assert_eq!(result.fills[1].price, 0.51);
        assert_eq!(result.fills[1].size, 6.0);
        assert_close(result.fills[0].fee_paid, 0.072);
        assert_close(result.fills[1].fee_paid, 0.107_956_8);
        assert_eq!(
            result.order.expect("filled order").status,
            PaperOrderStatus::Filled
        );
        assert!(result
            .fills
            .iter()
            .all(|fill| fill.liquidity == OrderKind::Taker));
    }

    #[test]
    fn taker_partial_fill_leaves_unfilled_remainder() {
        let mut executor = PaperExecutor::default();
        let book = book(
            vec![level(0.48, 100.0)],
            vec![level(0.50, 4.0), level(0.51, 2.0)],
            None,
        );

        let result = executor
            .open_paper_order(
                intent(OrderKind::Taker, Side::Buy, 0.52, 10.0),
                &approved(),
                &fee_parameters(),
                Some(&book),
                NOW,
            )
            .expect("taker order places");

        let order = result.order.expect("partial order");
        assert_eq!(order.filled_size, 6.0);
        assert_eq!(remaining_size(&order), 4.0);
        assert_eq!(order.status, PaperOrderStatus::PartiallyFilled);
        assert_eq!(result.fills.iter().map(|fill| fill.size).sum::<f64>(), 6.0);
    }

    #[test]
    fn open_order_can_be_canceled_with_audit_and_normalized_event() {
        let mut executor = PaperExecutor::default();
        let placed = executor
            .open_paper_order(
                intent(OrderKind::Maker, Side::Buy, 0.49, 10.0),
                &approved(),
                &fee_parameters(),
                Some(&book(vec![level(0.49, 1.0)], vec![], None)),
                NOW,
            )
            .expect("order places");
        let order_id = placed.order.expect("placed order").order_id;

        let canceled = executor
            .cancel_order(&order_id, "strategy canceled", NOW + 10)
            .expect("order cancels");

        assert_eq!(
            canceled.order.expect("canceled order").status,
            PaperOrderStatus::Canceled
        );
        assert!(matches!(
            canceled.audit_events[0],
            PaperExecutionAuditEvent::OrderCanceled { .. }
        ));
        assert_eq!(
            canceled.normalized_events[0].event_type(),
            crate::events::EventType::PaperOrderCanceled
        );
    }

    #[test]
    fn open_order_can_expire_with_audit_event() {
        let mut executor = PaperExecutor::default();
        let placed = executor
            .open_paper_order(
                intent(OrderKind::Maker, Side::Sell, 0.51, 10.0),
                &approved(),
                &fee_parameters(),
                Some(&book(vec![], vec![level(0.51, 5.0)], None)),
                NOW,
            )
            .expect("order places");
        let order_id = placed.order.expect("placed order").order_id;

        let expired = executor
            .expire_order(&order_id, "market window closed", NOW + 10)
            .expect("order expires");

        assert_eq!(
            expired.order.expect("expired order").status,
            PaperOrderStatus::Expired
        );
        assert!(matches!(
            expired.audit_events[0],
            PaperExecutionAuditEvent::OrderExpired { .. }
        ));
        assert!(expired.normalized_events.is_empty());
    }

    #[test]
    fn approved_intent_can_be_rejected_without_normalized_place_event() {
        let mut executor = PaperExecutor::default();

        let rejected = executor
            .reject_intent(
                intent(OrderKind::Maker, Side::Buy, 0.49, 10.0),
                &approved(),
                &fee_parameters(),
                "operator rejected",
                NOW,
            )
            .expect("intent rejects");

        let order = rejected.order.expect("rejected order");
        assert_eq!(order.status, PaperOrderStatus::Rejected);
        assert!(executor.order(&order.order_id).is_some());
        assert!(matches!(
            rejected.audit_events[1],
            PaperExecutionAuditEvent::OrderRejected { .. }
        ));
        assert!(rejected.normalized_events.is_empty());
    }

    #[test]
    fn risk_denied_intent_creates_no_paper_record() {
        let mut executor = PaperExecutor::default();

        let result = executor
            .open_paper_order(
                intent(OrderKind::Maker, Side::Buy, 0.49, 10.0),
                &denied(),
                &fee_parameters(),
                Some(&book(vec![level(0.49, 1.0)], vec![], None)),
                NOW,
            )
            .expect("risk denial is a paper result");

        assert!(result.order.is_none());
        assert!(result.fills.is_empty());
        assert!(executor.orders().is_empty());
        assert!(matches!(
            result.audit_events[0],
            PaperExecutionAuditEvent::RiskRejected { .. }
        ));
        assert!(result.normalized_events.is_empty());
    }

    fn intent(order_kind: OrderKind, side: Side, price: f64, size: f64) -> PaperOrderIntent {
        PaperOrderIntent {
            asset: Asset::Btc,
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            outcome: "Up".to_string(),
            side,
            order_kind,
            price,
            size,
            notional: price * size,
            fair_probability: 0.55,
            market_probability: price,
            expected_value_bps: 100.0,
            reason: "unit test".to_string(),
            required_inputs: vec!["book".to_string()],
            created_ts: NOW,
        }
    }

    fn approved() -> RiskGateDecision {
        RiskGateDecision {
            approved: true,
            violations: Vec::new(),
            risk_state: RiskState {
                halted: false,
                active_halts: Vec::new(),
                reason: None,
                updated_ts: NOW,
            },
        }
    }

    fn denied() -> RiskGateDecision {
        RiskGateDecision {
            approved: false,
            violations: vec![RiskViolation {
                reason: RiskHaltReason::MaxNotionalPerMarket,
                message: "market notional would exceed limit".to_string(),
            }],
            risk_state: RiskState {
                halted: true,
                active_halts: vec![RiskHaltReason::MaxNotionalPerMarket],
                reason: Some("market notional would exceed limit".to_string()),
                updated_ts: NOW,
            },
        }
    }

    fn fee_parameters() -> FeeParameters {
        FeeParameters {
            fees_enabled: true,
            maker_fee_bps: 0.0,
            taker_fee_bps: 1_000.0,
            raw_fee_config: Some(serde_json::json!({"r": 0.072, "e": 1, "to": true})),
        }
    }

    fn book(
        bids: Vec<PriceLevelSnapshot>,
        asks: Vec<PriceLevelSnapshot>,
        last_trade: Option<LastTradeState>,
    ) -> TokenBookSnapshot {
        TokenBookSnapshot {
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            best_bid: bids.first().map(|level| level.price),
            best_ask: asks.first().map(|level| level.price),
            spread: match (bids.first(), asks.first()) {
                (Some(bid), Some(ask)) => Some(ask.price - bid.price),
                _ => None,
            },
            bids: side(bids),
            asks: side(asks),
            last_update_ts: Some(NOW),
            last_recv_wall_ts: Some(NOW),
            hash: Some("book-hash".to_string()),
            last_trade,
        }
    }

    fn book_with_trade(side: Side, price: f64, size: f64, recv_wall_ts: i64) -> TokenBookSnapshot {
        book(
            vec![level(0.49, 100.0)],
            vec![level(0.51, 80.0)],
            Some(LastTradeState {
                side,
                price,
                size,
                fee_rate_bps: None,
                source_ts: Some(recv_wall_ts - 1),
                recv_wall_ts,
            }),
        )
    }

    fn side(levels: Vec<PriceLevelSnapshot>) -> BookSideSnapshot {
        BookSideSnapshot {
            visible_depth: levels.iter().map(|level| level.size).sum(),
            levels,
        }
    }

    fn level(price: f64, size: f64) -> PriceLevelSnapshot {
        PriceLevelSnapshot { price, size }
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1e-9,
            "actual={actual} expected={expected}"
        );
    }
}
