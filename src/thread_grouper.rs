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

pub fn group_by_thread(messages: Vec<EmailMessage>, verbose: bool) -> Vec<ConversationThread> {
    let mut thread_map: HashMap<String, (String, Vec<EmailMessage>)> = HashMap::new();

    for msg in messages {
        let key = msg.normalized_subject.clone();
        let display = msg.subject.clone();
        if verbose {
            if thread_map.contains_key(&key) {
                eprintln!("reading reply to {}, {}", msg.node_id, display);
            } else {
                eprintln!("reading message {}, {}", msg.node_id, display);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_msg(subject: &str, date_secs: Option<i64>) -> EmailMessage {
        EmailMessage {
            date: date_secs.map(|s| chrono::Utc.timestamp_opt(s, 0).unwrap()),
            from_name: String::new(),
            from_address: String::new(),
            to_recipients: vec![],
            subject: subject.to_string(),
            body: String::new(),
            normalized_subject: normalize_subject(subject),
            node_id: 0,
        }
    }

    // ── normalize_subject ────────────────────────────────────────────────────

    #[test]
    fn normalize_strips_re() {
        assert_eq!(normalize_subject("Re: Hello"), "hello");
    }

    #[test]
    fn normalize_strips_re_case_insensitive() {
        assert_eq!(normalize_subject("RE: Hello"), "hello");
        assert_eq!(normalize_subject("re: Hello"), "hello");
    }

    #[test]
    fn normalize_strips_fwd() {
        assert_eq!(normalize_subject("Fwd: Hello"), "hello");
        assert_eq!(normalize_subject("FWD: Hello"), "hello");
    }

    #[test]
    fn normalize_strips_fw() {
        assert_eq!(normalize_subject("FW: Hello"), "hello");
        assert_eq!(normalize_subject("fw: Hello"), "hello");
    }

    #[test]
    fn normalize_strips_nested_prefixes() {
        assert_eq!(normalize_subject("Re: Fwd: Re: Hello"), "hello");
        assert_eq!(normalize_subject("RE: RE: FW: subject"), "subject");
    }

    #[test]
    fn normalize_lowercases() {
        assert_eq!(normalize_subject("Hello World"), "hello world");
    }

    #[test]
    fn normalize_trims_whitespace() {
        assert_eq!(normalize_subject("  Re:  Hello  "), "hello");
    }

    #[test]
    fn normalize_empty() {
        assert_eq!(normalize_subject(""), "");
    }

    #[test]
    fn normalize_no_prefix_unchanged_except_case() {
        assert_eq!(normalize_subject("Meeting Tomorrow"), "meeting tomorrow");
    }

    // ── group_by_thread ──────────────────────────────────────────────────────

    #[test]
    fn group_empty_input() {
        let threads = group_by_thread(vec![], false);
        assert!(threads.is_empty());
    }

    #[test]
    fn group_single_message() {
        let msgs = vec![make_msg("Hello", Some(0))];
        let threads = group_by_thread(msgs, false);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].messages.len(), 1);
    }

    #[test]
    fn group_re_and_original_in_same_thread() {
        let msgs = vec![
            make_msg("Hello", Some(0)),
            make_msg("Re: Hello", Some(1)),
        ];
        let threads = group_by_thread(msgs, false);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].messages.len(), 2);
    }

    #[test]
    fn group_different_subjects_in_different_threads() {
        let msgs = vec![
            make_msg("Alpha", Some(0)),
            make_msg("Beta", Some(1)),
        ];
        let threads = group_by_thread(msgs, false);
        assert_eq!(threads.len(), 2);
    }

    #[test]
    fn group_messages_sorted_by_date_within_thread() {
        let msgs = vec![
            make_msg("Hello", Some(100)),
            make_msg("Re: Hello", Some(50)),
            make_msg("Re: Hello", Some(200)),
        ];
        let threads = group_by_thread(msgs, false);
        assert_eq!(threads.len(), 1);
        let dates: Vec<i64> = threads[0]
            .messages
            .iter()
            .map(|m| m.date.unwrap().timestamp())
            .collect();
        assert_eq!(dates, vec![50, 100, 200]);
    }

    #[test]
    fn group_threads_sorted_alphabetically() {
        let msgs = vec![
            make_msg("Zebra", Some(0)),
            make_msg("Apple", Some(0)),
            make_msg("Mango", Some(0)),
        ];
        let threads = group_by_thread(msgs, false);
        let subjects: Vec<&str> = threads.iter().map(|t| t.display_subject.as_str()).collect();
        assert_eq!(subjects, vec!["Apple", "Mango", "Zebra"]);
    }
}
