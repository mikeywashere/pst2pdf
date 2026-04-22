use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use outlook_pst::ltp::prop_context::PropertyValue;
use outlook_pst::ltp::table_context::TableRowColumnValue;
use outlook_pst::messaging::attachment::{
    AnsiAttachment, Attachment, AttachmentData, UnicodeAttachment,
};
use outlook_pst::messaging::folder::Folder;
use outlook_pst::messaging::message::{AnsiMessage, Message, UnicodeMessage};
use outlook_pst::messaging::store::{AnsiStore, EntryId, Store, UnicodeStore};
use outlook_pst::ndb::node_id::NodeId;
use outlook_pst::{AnsiPstFile, UnicodePstFile};

use crate::models::EmailMessage;
use crate::thread_grouper::normalize_subject;

const PR_SUBJECT: u16 = 0x0037;
const PR_SENDER_NAME: u16 = 0x0C1A;
const PR_SENDER_EMAIL_ADDRESS: u16 = 0x0C1F;
const PR_MESSAGE_DELIVERY_TIME: u16 = 0x0E06;
const PR_CLIENT_SUBMIT_TIME: u16 = 0x0039;
const PR_BODY: u16 = 0x1000;
const PR_HTML: u16 = 0x1013;
const PR_DISPLAY_NAME: u16 = 0x3001;
const PR_EMAIL_ADDRESS: u16 = 0x3003;
const PR_SMTP_ADDRESS: u16 = 0x39FE;
const PR_ATTACH_LONG_FILENAME: u16 = 0x3707;
const PR_ATTACH_FILENAME: u16 = 0x3704;

fn filetime_to_datetime(ft: i64) -> Option<DateTime<Utc>> {
    let unix_secs = (ft / 10_000_000) - 11_644_473_600;
    Utc.timestamp_opt(unix_secs, 0).single()
}

fn prop_to_string(pv: &PropertyValue) -> Option<String> {
    match pv {
        PropertyValue::String8(s) => Some(s.to_string()),
        PropertyValue::Unicode(s) => Some(s.to_string()),
        _ => None,
    }
}

fn prop_to_time(pv: &PropertyValue) -> Option<DateTime<Utc>> {
    if let PropertyValue::Time(ft) = pv {
        filetime_to_datetime(*ft)
    } else {
        None
    }
}

fn strip_html(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Decode a few common HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

pub fn read_messages(pst_path: &Path) -> Result<Vec<EmailMessage>> {
    let store = outlook_pst::open_store(pst_path)
        .with_context(|| format!("Failed to open PST file: {}", pst_path.display()))?;

    let root_entry_id = store
        .properties()
        .ipm_sub_tree_entry_id()
        .context("Failed to get IPM subtree entry ID")?;

    let root_folder = store
        .open_folder(&root_entry_id)
        .context("Failed to open root folder")?;

    let mut messages = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();

    traverse_folder(&store, &root_folder, &mut messages, &mut seen);

    Ok(messages)
}

fn traverse_folder(
    store: &Rc<dyn Store>,
    folder: &Rc<dyn Folder>,
    messages: &mut Vec<EmailMessage>,
    seen: &mut HashSet<u32>,
) {
    // Collect subfolder node IDs before dropping the hierarchy table borrow
    let subfolder_node_vals: Vec<u32> = match folder.hierarchy_table() {
        Some(hierarchy) => hierarchy
            .rows_matrix()
            .map(|r| u32::from(r.id()))
            .collect(),
        None => Vec::new(),
    };

    for node_val in subfolder_node_vals {
        let node_id = NodeId::from(node_val);
        if let Ok(entry_id) = store.properties().make_entry_id(node_id) {
            if let Ok(subfolder) = store.open_folder(&entry_id) {
                traverse_folder(store, &subfolder, messages, seen);
            }
        }
    }

    // Collect message node IDs before dropping the contents table borrow
    let message_node_vals: Vec<u32> = match folder.contents_table() {
        Some(contents) => contents
            .rows_matrix()
            .map(|r| u32::from(r.id()))
            .collect(),
        None => Vec::new(),
    };

    for node_val in message_node_vals {
        let node_id = NodeId::from(node_val);
        if let Ok(entry_id) = store.properties().make_entry_id(node_id) {
            // Deduplicate by node ID
            let dedup_key = u32::from(entry_id.node_id());
            if seen.contains(&dedup_key) {
                continue;
            }
            seen.insert(dedup_key);

            if let Ok(msg) = store.open_message(&entry_id, None) {
                if let Ok(email) = extract_message(&msg, node_val) {
                    messages.push(email);
                }
            }
        }
    }
}

fn extract_recipients(msg: &Rc<dyn Message>) -> Vec<String> {
    let mut recipients = Vec::new();

    let table = match msg.recipient_table() {
        Some(t) => t,
        None => return recipients,
    };

    let context = table.context();
    let columns = context.columns().to_vec();

    let row_ids: Vec<u32> = table.rows_matrix().map(|r| u32::from(r.id())).collect();

    for row_id_val in row_ids {
        let row_id = outlook_pst::ltp::table_context::TableRowId::new(row_id_val);
        let row = match table.find_row(row_id) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let col_values = match row.columns(context) {
            Ok(cv) => cv,
            Err(_) => continue,
        };

        let mut display_name: Option<String> = None;
        let mut email_addr: Option<String> = None;

        for (i, col_value_opt) in col_values.iter().enumerate() {
            if i >= columns.len() {
                break;
            }
            if let Some(col_value) = col_value_opt {
                let col = &columns[i];
                let pv = match col_value {
                    TableRowColumnValue::Small(pv) => pv.clone(),
                    TableRowColumnValue::Heap(_) | TableRowColumnValue::Node(_) => {
                        match table.read_column(col_value, col.prop_type()) {
                            Ok(pv) => pv,
                            Err(_) => continue,
                        }
                    }
                };
                match col.prop_id() {
                    id if id == PR_DISPLAY_NAME => {
                        display_name = prop_to_string(&pv);
                    }
                    id if id == PR_EMAIL_ADDRESS || id == PR_SMTP_ADDRESS => {
                        if email_addr.is_none() {
                            email_addr = prop_to_string(&pv);
                        }
                    }
                    _ => {}
                }
            }
        }

        match (display_name, email_addr) {
            (Some(name), Some(email)) if !email.is_empty() => {
                recipients.push(format!("{} <{}>", name, email));
            }
            (Some(name), _) => {
                recipients.push(name);
            }
            (None, Some(email)) => {
                recipients.push(email);
            }
            _ => {}
        }
    }

    recipients
}

fn extract_message(msg: &Rc<dyn Message>, node_id: u32) -> Result<EmailMessage> {
    let props = msg.properties();

    let subject = props
        .get(PR_SUBJECT)
        .and_then(prop_to_string)
        .unwrap_or_default();

    let from_name = props
        .get(PR_SENDER_NAME)
        .and_then(prop_to_string)
        .unwrap_or_default();

    let from_address = props
        .get(PR_SENDER_EMAIL_ADDRESS)
        .and_then(prop_to_string)
        .unwrap_or_default();

    let date = props
        .get(PR_MESSAGE_DELIVERY_TIME)
        .and_then(prop_to_time)
        .or_else(|| props.get(PR_CLIENT_SUBMIT_TIME).and_then(prop_to_time))
        .or_else(|| {
            props
                .creation_time()
                .ok()
                .and_then(filetime_to_datetime)
        });

    let body = props
        .get(PR_BODY)
        .and_then(prop_to_string)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            props
                .get(PR_HTML)
                .and_then(prop_to_string)
                .map(|html| strip_html(&html))
        })
        .unwrap_or_default();

    let to_recipients = extract_recipients(msg);

    let normalized_subject = normalize_subject(&subject);

    Ok(EmailMessage {
        date,
        from_name,
        from_address,
        to_recipients,
        subject,
        body,
        normalized_subject,
        node_id,
    })
}

// ── Attachment extraction ─────────────────────────────────────────────────────

pub fn save_attachments(pst_path: &Path, output_dir: &Path) -> Result<usize> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create directory: {}", output_dir.display()))?;

    let mut used_names: HashSet<String> = HashSet::new();

    // Try Unicode PST first, fall back to ANSI
    if let Ok(pst_file) = UnicodePstFile::open(pst_path) {
        let store = UnicodeStore::read(Rc::new(pst_file))
            .with_context(|| format!("Failed to open Unicode PST store: {}", pst_path.display()))?;

        let dyn_store: Rc<dyn Store> = store.clone();
        let entry_ids = collect_message_entry_ids(&dyn_store);
        let mut saved = 0usize;
        for entry_id in &entry_ids {
            if let Ok(msg) = UnicodeMessage::read(store.clone(), entry_id, None) {
                saved += save_unicode_message_attachments(&msg, output_dir, &mut used_names, "");
            }
        }
        return Ok(saved);
    }

    let pst_file = AnsiPstFile::open(pst_path)
        .with_context(|| format!("Failed to open PST file: {}", pst_path.display()))?;
    let store = AnsiStore::read(Rc::new(pst_file))
        .with_context(|| format!("Failed to open ANSI PST store: {}", pst_path.display()))?;

    let dyn_store: Rc<dyn Store> = store.clone();
    let entry_ids = collect_message_entry_ids(&dyn_store);
    let mut saved = 0usize;
    for entry_id in &entry_ids {
        if let Ok(msg) = AnsiMessage::read(store.clone(), entry_id, None) {
            saved += save_ansi_message_attachments(&msg, output_dir, &mut used_names, "");
        }
    }
    Ok(saved)
}

/// Save attachments for all messages, prefixing each attachment filename with
/// the 1-based conversation index so attachments can be correlated to their PDF.
/// Called when `--conversations` and `--attachments` are both set.
pub fn save_attachments_for_threads(
    pst_path: &Path,
    output_dir: &Path,
    threads: &[crate::models::ConversationThread],
    stem: &str,
) -> Result<usize> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create directory: {}", output_dir.display()))?;

    // Build map from message node_id → 1-based thread index
    let mut nid_to_thread: HashMap<u32, usize> = HashMap::new();
    for (i, thread) in threads.iter().enumerate() {
        for msg in &thread.messages {
            nid_to_thread.insert(msg.node_id, i + 1);
        }
    }

    let mut used_names: HashSet<String> = HashSet::new();

    if let Ok(pst_file) = UnicodePstFile::open(pst_path) {
        let store = UnicodeStore::read(Rc::new(pst_file))
            .with_context(|| format!("Failed to open Unicode PST store: {}", pst_path.display()))?;
        let dyn_store: Rc<dyn Store> = store.clone();
        let entry_ids = collect_message_entry_ids(&dyn_store);
        let mut saved = 0usize;
        for entry_id in &entry_ids {
            let nid = u32::from(entry_id.node_id());
            if let Some(&thread_idx) = nid_to_thread.get(&nid) {
                let prefix = format!("{}-{:05}-", stem, thread_idx);
                if let Ok(msg) = UnicodeMessage::read(store.clone(), entry_id, None) {
                    saved += save_unicode_message_attachments(&msg, output_dir, &mut used_names, &prefix);
                }
            }
        }
        return Ok(saved);
    }

    let pst_file = AnsiPstFile::open(pst_path)
        .with_context(|| format!("Failed to open PST file: {}", pst_path.display()))?;
    let store = AnsiStore::read(Rc::new(pst_file))
        .with_context(|| format!("Failed to open ANSI PST store: {}", pst_path.display()))?;
    let dyn_store: Rc<dyn Store> = store.clone();
    let entry_ids = collect_message_entry_ids(&dyn_store);
    let mut saved = 0usize;
    for entry_id in &entry_ids {
        let nid = u32::from(entry_id.node_id());
        if let Some(&thread_idx) = nid_to_thread.get(&nid) {
            let prefix = format!("{}-{:05}-", stem, thread_idx);
            if let Ok(msg) = AnsiMessage::read(store.clone(), entry_id, None) {
                saved += save_ansi_message_attachments(&msg, output_dir, &mut used_names, &prefix);
            }
        }
    }
    Ok(saved)
}

/// Recursively collect EntryIds for every message in the PST, deduplicating by NID.
fn collect_message_entry_ids(store: &Rc<dyn Store>) -> Vec<EntryId> {
    let root_entry_id = match store.properties().ipm_sub_tree_entry_id() {
        Ok(id) => id,
        Err(_) => return Vec::new(),
    };
    let root_folder = match store.open_folder(&root_entry_id) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let mut entry_ids = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();
    collect_folder_messages(store, &root_folder, &mut entry_ids, &mut seen);
    entry_ids
}

fn collect_folder_messages(
    store: &Rc<dyn Store>,
    folder: &Rc<dyn Folder>,
    entry_ids: &mut Vec<EntryId>,
    seen: &mut HashSet<u32>,
) {
    let subfolder_vals: Vec<u32> = match folder.hierarchy_table() {
        Some(h) => h.rows_matrix().map(|r| u32::from(r.id())).collect(),
        None => Vec::new(),
    };
    for val in subfolder_vals {
        if let Ok(entry_id) = store.properties().make_entry_id(NodeId::from(val)) {
            if let Ok(subfolder) = store.open_folder(&entry_id) {
                collect_folder_messages(store, &subfolder, entry_ids, seen);
            }
        }
    }

    let msg_vals: Vec<u32> = match folder.contents_table() {
        Some(c) => c.rows_matrix().map(|r| u32::from(r.id())).collect(),
        None => Vec::new(),
    };
    for val in msg_vals {
        let node_id = NodeId::from(val);
        if let Ok(entry_id) = store.properties().make_entry_id(node_id) {
            let key = u32::from(entry_id.node_id());
            if !seen.contains(&key) {
                seen.insert(key);
                entry_ids.push(entry_id);
            }
        }
    }
}

/// Extract attachments from a Unicode message using UnicodeAttachment::read(),
/// which correctly opens each attachment sub-node from the message's sub-node tree.
fn save_unicode_message_attachments(
    msg: &Rc<UnicodeMessage>,
    output_dir: &Path,
    used_names: &mut HashSet<String>,
    prefix: &str,
) -> usize {
    let table = match msg.attachment_table() {
        Some(t) => t,
        None => return 0,
    };

    // Per MS-PST spec §2.4.6.1.3: each row's dwRowID is the subnode NID of the
    // attachment object PC. Use UnicodeAttachment::read() to open it properly.
    let attachment_nids: Vec<u32> = table.rows_matrix().map(|r| u32::from(r.id())).collect();
    let mut count = 0usize;

    for nid_val in attachment_nids {
        let sub_node = NodeId::from(nid_val);
        let attachment = match UnicodeAttachment::read(msg.clone(), sub_node, None) {
            Ok(a) => a,
            Err(_) => continue,
        };
        if write_attachment(attachment.data(), attachment.properties(), output_dir, used_names, prefix) {
            count += 1;
        }
    }
    count
}

/// Same as above but for ANSI PST files.
fn save_ansi_message_attachments(
    msg: &Rc<AnsiMessage>,
    output_dir: &Path,
    used_names: &mut HashSet<String>,
    prefix: &str,
) -> usize {
    let table = match msg.attachment_table() {
        Some(t) => t,
        None => return 0,
    };

    let attachment_nids: Vec<u32> = table.rows_matrix().map(|r| u32::from(r.id())).collect();
    let mut count = 0usize;

    for nid_val in attachment_nids {
        let sub_node = NodeId::from(nid_val);
        let attachment = match AnsiAttachment::read(msg.clone(), sub_node, None) {
            Ok(a) => a,
            Err(_) => continue,
        };
        if write_attachment(attachment.data(), attachment.properties(), output_dir, used_names, prefix) {
            count += 1;
        }
    }
    count
}

fn write_attachment(
    data: Option<&AttachmentData>,
    props: &outlook_pst::messaging::attachment::AttachmentProperties,
    output_dir: &Path,
    used_names: &mut HashSet<String>,
    prefix: &str,
) -> bool {
    let bytes = match data {
        Some(AttachmentData::Binary(bv)) => bv.buffer(),
        _ => return false,
    };
    if bytes.is_empty() {
        return false;
    }

    let raw_name = props
        .get(PR_ATTACH_LONG_FILENAME)
        .and_then(prop_to_string)
        .or_else(|| props.get(PR_ATTACH_FILENAME).and_then(prop_to_string))
        .unwrap_or_else(|| "attachment.bin".to_string());

    let prefixed_name = format!("{}{}", prefix, raw_name);
    let filename = unique_filename(&prefixed_name, used_names);
    let dest = output_dir.join(&filename);
    std::fs::write(&dest, bytes).is_ok()
}

fn unique_filename(name: &str, used_names: &mut HashSet<String>) -> String {
    let safe: String = name
        .chars()
        .map(|c| if c == '/' || c == '\\' || c == '\0' { '_' } else { c })
        .collect();
    let safe = safe.trim().to_string();
    let safe = if safe.is_empty() {
        "attachment.bin".to_string()
    } else {
        safe
    };

    if !used_names.contains(&safe) {
        used_names.insert(safe.clone());
        return safe;
    }
    let (stem, ext) = match safe.rfind('.') {
        Some(dot) => (&safe[..dot], &safe[dot..]),
        None => (safe.as_str(), ""),
    };
    let mut counter = 1u32;
    loop {
        let candidate = format!("{}_{}{}", stem, counter, ext);
        if !used_names.contains(&candidate) {
            used_names.insert(candidate.clone());
            return candidate;
        }
        counter += 1;
    }
}
