use std::collections::HashMap;
use std::collections::HashSet;

use crate::models::{ConversationThread, EmailMessage};

fn display_subject(subject: &str) -> String {
    let trimmed = subject.trim();
    if trimmed.is_empty() {
        "(no subject)".to_string()
    } else {
        trimmed.to_string()
    }
}

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
    let mut id_to_index: HashMap<String, usize> = HashMap::new();
    for (idx, msg) in messages.iter().enumerate() {
        if let Some(id) = msg.message_id.as_ref() {
            id_to_index.insert(id.clone(), idx);
        }
    }

    let mut resolved_keys: HashMap<usize, String> = HashMap::new();
    let mut resolved_depths: HashMap<usize, usize> = HashMap::new();
    let mut visiting: HashSet<usize> = HashSet::new();

    for idx in 0..messages.len() {
        let msg = &messages[idx];
        let key = resolve_thread_key(
            idx,
            &messages,
            &id_to_index,
            &mut resolved_keys,
            &mut visiting,
        );
        let depth = resolve_reply_depth(
            idx,
            &messages,
            &id_to_index,
            &mut resolved_depths,
            &mut visiting,
        );
        let mut msg = msg.clone();
        msg.reply_depth = depth;
        let display = display_subject(&msg.subject);
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

fn thread_parent_id(msg: &EmailMessage) -> Option<String> {
    msg.in_reply_to
        .clone()
        .or_else(|| msg.references.last().cloned())
}

fn resolve_thread_key(
    idx: usize,
    messages: &[EmailMessage],
    id_to_index: &HashMap<String, usize>,
    cache: &mut HashMap<usize, String>,
    visiting: &mut HashSet<usize>,
) -> String {
    if let Some(key) = cache.get(&idx) {
        return key.clone();
    }
    if !visiting.insert(idx) {
        return fallback_thread_key(&messages[idx]);
    }

    let msg = &messages[idx];
    let key = if msg.normalized_subject.trim().is_empty() {
        if let Some(parent_id) = thread_parent_id(msg) {
            if let Some(&parent_idx) = id_to_index.get(&parent_id) {
                resolve_thread_key(parent_idx, messages, id_to_index, cache, visiting)
            } else {
                fallback_thread_key(msg)
            }
        } else {
            fallback_thread_key(msg)
        }
    } else if let Some(parent_id) = thread_parent_id(msg) {
        if let Some(&parent_idx) = id_to_index.get(&parent_id) {
            resolve_thread_key(parent_idx, messages, id_to_index, cache, visiting)
        } else {
            msg.normalized_subject.clone()
        }
    } else {
        msg.normalized_subject.clone()
    };

    visiting.remove(&idx);
    cache.insert(idx, key.clone());
    key
}

fn resolve_reply_depth(
    idx: usize,
    messages: &[EmailMessage],
    id_to_index: &HashMap<String, usize>,
    cache: &mut HashMap<usize, usize>,
    visiting: &mut HashSet<usize>,
) -> usize {
    if let Some(depth) = cache.get(&idx) {
        return *depth;
    }
    if !visiting.insert(idx) {
        return 0;
    }

    let msg = &messages[idx];
    let depth = if let Some(parent_id) = thread_parent_id(msg) {
        if let Some(&parent_idx) = id_to_index.get(&parent_id) {
            resolve_reply_depth(parent_idx, messages, id_to_index, cache, visiting) + 1
        } else {
            0
        }
    } else {
        0
    };

    visiting.remove(&idx);
    cache.insert(idx, depth);
    depth
}

fn fallback_thread_key(msg: &EmailMessage) -> String {
    if msg.normalized_subject.trim().is_empty() {
        format!("__no_subject__{}", msg.node_id)
    } else {
        msg.normalized_subject.clone()
    }
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
            message_id: None,
            in_reply_to: None,
            references: vec![],
            reply_depth: 0,
            node_id: 0,
        }
    }

    fn make_msg_with_id(subject: &str, date_secs: Option<i64>, node_id: u32) -> EmailMessage {
        let mut msg = make_msg(subject, date_secs);
        msg.node_id = node_id;
        msg
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
    fn group_empty_subjects_separately() {
        let msgs = vec![
            make_msg_with_id("", Some(0), 10),
            make_msg_with_id("", Some(1), 11),
        ];
        let threads = group_by_thread(msgs, false);
        assert_eq!(threads.len(), 2);
    }

    #[test]
    fn group_reply_to_header_keeps_thread_together() {
        let mut parent = make_msg_with_id("Hello", Some(0), 10);
        parent.message_id = Some("<msg-1@example.com>".to_string());
        let mut reply = make_msg_with_id("", Some(1), 11);
        reply.in_reply_to = Some("<msg-1@example.com>".to_string());
        let threads = group_by_thread(vec![parent, reply], false);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].messages[0].reply_depth, 0);
        assert_eq!(threads[0].messages[1].reply_depth, 1);
    }

    #[test]
    fn group_nested_reply_depth_increases() {
        let mut parent = make_msg_with_id("Hello", Some(0), 10);
        parent.message_id = Some("<msg-1@example.com>".to_string());
        let mut child = make_msg_with_id("Re: Hello", Some(1), 11);
        child.message_id = Some("<msg-2@example.com>".to_string());
        child.in_reply_to = Some("<msg-1@example.com>".to_string());
        let mut grandchild = make_msg_with_id("", Some(2), 12);
        grandchild.in_reply_to = Some("<msg-2@example.com>".to_string());
        let threads = group_by_thread(vec![parent, child, grandchild], false);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].messages[0].reply_depth, 0);
        assert_eq!(threads[0].messages[1].reply_depth, 1);
        assert_eq!(threads[0].messages[2].reply_depth, 2);
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
