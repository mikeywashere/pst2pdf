use std::collections::HashMap;

use crate::models::{ConversationThread, EmailMessage};

pub fn normalize_subject(subject: &str) -> String {
    let mut s = subject.trim().to_string();
    loop {
        let lower = s.to_lowercase();
        if lower.starts_with("re:") {
            s = s[3..].trim().to_string();
        } else if lower.starts_with("fwd:") {
            s = s[4..].trim().to_string();
        } else if lower.starts_with("fw:") {
            s = s[3..].trim().to_string();
        } else {
            break;
        }
    }
    s.to_lowercase()
}

pub fn group_by_thread(messages: Vec<EmailMessage>) -> Vec<ConversationThread> {
    let mut thread_map: HashMap<String, (String, Vec<EmailMessage>)> = HashMap::new();

    for msg in messages {
        let key = msg.normalized_subject.clone();
        let display = msg.subject.clone();
        let entry = thread_map
            .entry(key)
            .or_insert_with(|| (display, Vec::new()));
        entry.1.push(msg);
    }

    let mut threads: Vec<ConversationThread> = thread_map
        .into_iter()
        .map(|(normalized_subject, (display_subject, mut messages))| {
            messages.sort_by(|a, b| a.date.cmp(&b.date));
            ConversationThread {
                normalized_subject,
                display_subject,
                messages,
            }
        })
        .collect();

    threads.sort_by(|a, b| a.display_subject.cmp(&b.display_subject));
    threads
}
