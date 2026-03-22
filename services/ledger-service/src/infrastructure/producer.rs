use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use metrics::counter;
use rdkafka::{
    config::ClientConfig,
    producer::{FutureProducer, FutureRecord},
};
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use crate::application::dto::trade::TradeResponse;

// ─── Event schema ─────────────────────────────────────────────────────────────

/// JSON payload published to the `trades` topic on every successful trade creation.
#[derive(Debug, Serialize)]
pub struct TradeEvent {
    /// Unique ID for this event (idempotency / deduplication key for consumers).
    pub event_id: Uuid,
    pub event_type: &'static str,
    pub trade_id: Uuid,
    pub asset: String,
    pub quantity: Decimal,
    pub price: Decimal,
    pub side: String,
    pub notional_value: Decimal,
    pub realized_pnl: Decimal,
    /// When the trade was executed.
    pub timestamp: DateTime<Utc>,
    /// When this event was emitted.
    pub published_at: DateTime<Utc>,
}

impl TradeEvent {
    pub fn from_response(resp: &TradeResponse) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            event_type: "trade_created",
            trade_id: resp.id,
            asset: resp.asset.clone(),
            quantity: resp.quantity,
            price: resp.price,
            side: resp.side.clone(),
            notional_value: resp.notional_value,
            realized_pnl: resp.realized_pnl,
            timestamp: resp.timestamp,
            published_at: Utc::now(),
        }
    }
}

// ─── Producer ─────────────────────────────────────────────────────────────────

const MAX_RETRIES: u32 = 3;
/// Base delay for exponential backoff: 100 ms → 200 ms → 400 ms.
const BASE_BACKOFF_MS: u64 = 100;
/// Per-send delivery timeout passed to rdkafka.
const SEND_TIMEOUT: Duration = Duration::from_secs(5);

pub struct KafkaTradeProducer {
    producer: FutureProducer,
    topic: String,
}

impl KafkaTradeProducer {
    /// Build a producer connected to `brokers` and targeting `topic`.
    /// Returns an error if rdkafka cannot initialise the client.
    pub fn new(brokers: &str, topic: impl Into<String>) -> Result<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            // Let rdkafka handle its own internal retries for transient network
            // errors; application-level retries below handle producer-queue
            // saturation and other delivery failures.
            .set("retries", "3")
            .create()?;

        Ok(Self {
            producer,
            topic: topic.into(),
        })
    }

    /// Serialize `event` to JSON and publish it.
    ///
    /// Retries up to [`MAX_RETRIES`] times with exponential backoff.  After all
    /// attempts are exhausted the error is logged and the call returns — trade
    /// creation is *not* rolled back.
    pub async fn publish_trade_created(&self, event: &TradeEvent) {
        let payload = match serde_json::to_string(event) {
            Ok(p) => p,
            Err(err) => {
                tracing::error!(trade_id = %event.trade_id, %err, "failed to serialize trade event");
                return;
            }
        };
        let key = event.trade_id.to_string();

        for attempt in 1..=MAX_RETRIES {
            let record = FutureRecord::to(&self.topic)
                .payload(&payload)
                .key(&key);

            match self.producer.send(record, SEND_TIMEOUT).await {
                Ok((partition, offset)) => {
                    counter!("ledger_kafka_publish_total", "status" => "success").increment(1);
                    tracing::info!(
                        trade_id = %event.trade_id,
                        topic = %self.topic,
                        partition,
                        offset,
                        "trade event published"
                    );
                    return;
                }
                Err((err, _msg)) => {
                    if attempt == MAX_RETRIES {
                        counter!("ledger_kafka_publish_total", "status" => "failure").increment(1);
                        tracing::error!(
                            trade_id = %event.trade_id,
                            %err,
                            attempt,
                            "trade event publish failed after all retries"
                        );
                    } else {
                        let backoff = Duration::from_millis(BASE_BACKOFF_MS * (1 << (attempt - 1)));
                        tracing::warn!(
                            trade_id = %event.trade_id,
                            %err,
                            attempt,
                            backoff_ms = backoff.as_millis(),
                            "trade event publish failed, retrying"
                        );
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }
    }
}
