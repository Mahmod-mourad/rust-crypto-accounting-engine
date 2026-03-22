use anyhow::Result;
use metrics::counter;
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    ClientConfig, Message,
};

use crate::{
    application::processor::EventProcessor,
    config::KafkaConfig,
    domain::trade_event::TradeEvent,
};

// ─── KafkaConsumer ────────────────────────────────────────────────────────────

pub struct KafkaConsumer {
    inner: StreamConsumer,
    topic: String,
    processor: EventProcessor,
}

impl KafkaConsumer {
    pub fn new(cfg: &KafkaConfig, processor: EventProcessor) -> Result<Self> {
        let inner: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &cfg.brokers)
            .set("group.id", &cfg.group_id)
            // Manual offset commit — we only advance the offset after a
            // successful DB commit so no event is silently dropped on crash.
            .set("enable.auto.commit", "false")
            .set("auto.offset.reset", &cfg.offset_reset)
            // These timeouts must be generous enough for slow DB commits.
            .set("session.timeout.ms", "30000")
            .set("max.poll.interval.ms", "300000")
            .create()?;

        inner.subscribe(&[cfg.topic.as_str()])?;

        Ok(Self {
            inner,
            topic: cfg.topic.clone(),
            processor,
        })
    }

    /// Block until the process is killed or an unrecoverable error occurs.
    ///
    /// Processing guarantee: **at-least-once delivery** at the Kafka level,
    /// promoted to **effectively-once** by the DB-side idempotency check in
    /// `EventProcessor`.
    pub async fn run(self) -> Result<()> {
        tracing::info!(topic = %self.topic, "kafka consumer started, waiting for messages");

        loop {
            match self.inner.recv().await {
                Err(e) => {
                    // rdkafka errors are generally transient (rebalance, timeout).
                    // Log and keep running — the consumer will recover.
                    tracing::error!(error = %e, "kafka receive error");
                }

                Ok(msg) => {
                    let partition = msg.partition();
                    let offset = msg.offset();
                    counter!("pnl_consumer_kafka_messages_total").increment(1);

                    // ── Deserialize ───────────────────────────────────────────
                    let payload = match msg.payload() {
                        Some(p) => p,
                        None => {
                            tracing::warn!(partition, offset, "empty kafka message, skipping");
                            // Commit so we don't re-read this slot.
                            let _ = self.inner.commit_message(&msg, CommitMode::Async);
                            continue;
                        }
                    };

                    let event: TradeEvent = match serde_json::from_slice(payload) {
                        Ok(e) => e,
                        Err(e) => {
                            tracing::error!(
                                partition,
                                offset,
                                error = %e,
                                payload = %String::from_utf8_lossy(payload),
                                "failed to deserialize trade event — skipping (poison pill)"
                            );
                            // Commit so the poison pill doesn't block the partition.
                            let _ = self.inner.commit_message(&msg, CommitMode::Async);
                            continue;
                        }
                    };

                    // ── Process ───────────────────────────────────────────────
                    match self
                        .processor
                        .process(&event, partition, offset)
                        .await
                    {
                        Ok(true) => {
                            tracing::debug!(
                                event_id = %event.event_id,
                                partition,
                                offset,
                                "event processed — committing offset"
                            );
                            let _ = self.inner.commit_message(&msg, CommitMode::Async);
                        }
                        Ok(false) => {
                            // Duplicate — nothing was written, but we still
                            // advance the offset to avoid re-reading forever.
                            tracing::debug!(
                                event_id = %event.event_id,
                                partition,
                                offset,
                                "duplicate event — skipping and committing offset"
                            );
                            let _ = self.inner.commit_message(&msg, CommitMode::Async);
                        }
                        Err(e) => {
                            // Processing failed.  Do NOT commit the offset so
                            // Kafka redelivers the message after a session
                            // timeout or rebalance.
                            counter!("pnl_consumer_events_total", "status" => "error").increment(1);
                            tracing::error!(
                                event_id = %event.event_id,
                                partition,
                                offset,
                                error = %e,
                                "event processing failed — offset NOT committed, will retry"
                            );
                        }
                    }
                }
            }
        }
    }
}
