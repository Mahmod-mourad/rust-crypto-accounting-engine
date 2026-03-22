use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a ledger account holding a crypto balance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub currency: String,
    pub balance: i64, // stored in smallest unit (e.g. satoshis)
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The direction of a ledger entry relative to an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EntryKind {
    Debit,
    Credit,
}

/// A double-entry ledger transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: Uuid,
    pub reference: String,
    pub description: Option<String>,
    pub entries: Vec<LedgerEntry>,
    pub created_at: DateTime<Utc>,
}

/// A single entry in a double-entry transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub account_id: Uuid,
    pub kind: EntryKind,
    pub amount: i64,
    pub currency: String,
    pub created_at: DateTime<Utc>,
}
