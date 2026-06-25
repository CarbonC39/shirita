//! Scrolling summary: Overwrites the historical compressed text from atart of conversation → cutoff_message_id (inclusive).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub id: String,
    pub session_id: String,
    pub cutoff_message_id: String,
    pub content: String,
    pub created_at: String,
}

impl Summary {
    pub fn new(session_id: &str, cutoff_message_id: &str, content: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            cutoff_message_id: cutoff_message_id.to_string(),
            content: content.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}
