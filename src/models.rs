use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub date: Option<DateTime<Utc>>,
    pub from_name: String,
    pub from_address: String,
    pub to_recipients: Vec<String>,
    pub subject: String,
    pub body: String,
    pub normalized_subject: String,
}

#[derive(Debug)]
pub struct ConversationThread {
    pub normalized_subject: String,
    pub display_subject: String,
    pub messages: Vec<EmailMessage>,
}
