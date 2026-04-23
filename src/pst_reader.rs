use std::collections::{HashMap, HashSet};
use std::io::Read;
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

    let lower = raw_name.to_lowercase();

    if lower.ends_with(".eml") {
        return write_eml_attachment(bytes, &raw_name, output_dir, used_names, prefix);
    }

    if lower.ends_with(".emz") {
        return write_emz_attachment(bytes, &raw_name, output_dir, used_names, prefix);
    }

    let prefixed_name = format!("{}{}", prefix, raw_name);
    let filename = unique_filename(&prefixed_name, used_names);
    let dest = output_dir.join(&filename);
    std::fs::write(&dest, bytes).is_ok()
}

/// Decode an `.eml` attachment to PDF. Falls back to saving the raw bytes if
/// parsing or rendering fails so the attachment is never silently dropped.
fn write_eml_attachment(
    bytes: &[u8],
    raw_name: &str,
    output_dir: &Path,
    used_names: &mut HashSet<String>,
    prefix: &str,
) -> bool {
    let stem = std::path::Path::new(raw_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(raw_name);

    if let Some(pdf_bytes) = eml_bytes_to_pdf(bytes) {
        let pdf_name = format!("{}{}.pdf", prefix, stem);
        let filename = unique_filename(&pdf_name, used_names);
        let dest = output_dir.join(&filename);
        if std::fs::write(&dest, pdf_bytes).is_ok() {
            return true;
        }
    }

    // Fallback: save original bytes
    let orig = format!("{}{}", prefix, raw_name);
    let filename = unique_filename(&orig, used_names);
    let dest = output_dir.join(&filename);
    std::fs::write(&dest, bytes).is_ok()
}

/// Maximum bytes we will decompress into RAM for EML→PDF conversion (50 MB).
/// Content larger than this is streamed directly to disk instead.
const MAX_EML_BYTES: u64 = 50 * 1024 * 1024;

/// Decompress an `.emz` archive, locate the first `.eml` entry, decode it to
/// PDF, and save it.
///
/// Large content is streamed directly to disk to avoid OOM. The fallback
/// chain is:
///   0. raw OLE-wrapped EMF (no compression) → strip wrapper → save .emf
///   1. zip → .eml found, small  → PDF
///   2. zip → .eml found, large  → stream raw .eml to disk
///   3. gzip → EML heuristic, small → PDF
///   4. gzip → EML heuristic, large → stream decompressed .eml to disk
///   5. gzip → OLE-wrapped EMF, small → strip wrapper → save .emf
///   6. gzip → OLE-wrapped EMF, large → stream decompressed .emf to disk
///   7. gzip → other content → save/stream decompressed to disk
///   8. everything failed → save original .emz bytes
fn write_emz_attachment(
    bytes: &[u8],
    raw_name: &str,
    output_dir: &Path,
    used_names: &mut HashSet<String>,
    prefix: &str,
) -> bool {
    let stem = std::path::Path::new(raw_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(raw_name);

    // ── 0: raw OLE-wrapped EMF (no compression) ──────────────────────────────
    if let Some(emf_data) = strip_emf_wrapper(bytes) {
        let name = unique_filename(&format!("{}{}.emf", prefix, stem), used_names);
        if std::fs::write(output_dir.join(&name), emf_data).is_ok() {
            return true;
        }
    }

    // ── 1 & 2: zip path ──────────────────────────────────────────────────────
    if zip_has_eml(bytes) {
        if let Some(eml_data) = try_zip_find_eml(bytes) {
            // Small enough – try PDF conversion.
            if let Some(pdf_bytes) = eml_bytes_to_pdf(&eml_data) {
                let name = unique_filename(&format!("{}{}.pdf", prefix, stem), used_names);
                if std::fs::write(output_dir.join(&name), pdf_bytes).is_ok() {
                    return true;
                }
            }
            // PDF failed – save as raw .eml.
            let name = unique_filename(&format!("{}{}.eml", prefix, stem), used_names);
            if std::fs::write(output_dir.join(&name), &eml_data).is_ok() {
                return true;
            }
        } else {
            // Entry exists but is too large – stream directly to disk.
            let name = unique_filename(&format!("{}{}.eml", prefix, stem), used_names);
            if zip_stream_eml_to_file(bytes, &output_dir.join(&name)) {
                return true;
            }
        }
    }

    // ── 3, 4 & 5: gzip path ──────────────────────────────────────────────────
    if is_gzip_magic(bytes) {
        // Peek at a small prefix to decide what the decompressed content is.
        let prefix_data = gunzip_prefix(bytes).unwrap_or_default();

        if looks_like_eml(&prefix_data) {
            // Try to decompress within the memory budget.
            if let Some(eml_data) = try_gunzip_limited(bytes) {
                // Small enough – try PDF conversion.
                if let Some(pdf_bytes) = eml_bytes_to_pdf(&eml_data) {
                    let name = unique_filename(&format!("{}{}.pdf", prefix, stem), used_names);
                    if std::fs::write(output_dir.join(&name), pdf_bytes).is_ok() {
                        return true;
                    }
                }
                // PDF failed – save decompressed bytes as .eml.
                let name = unique_filename(&format!("{}{}.eml", prefix, stem), used_names);
                if std::fs::write(output_dir.join(&name), &eml_data).is_ok() {
                    return true;
                }
            } else {
                // Too large – stream decompressed bytes to disk as .eml.
                let name = unique_filename(&format!("{}{}.eml", prefix, stem), used_names);
                if gunzip_to_file(bytes, &output_dir.join(&name)) {
                    return true;
                }
            }
        } else {
            // Not EML – try bounded decompression to check for EMF wrapper.
            if let Some(decompressed) = try_gunzip_limited(bytes) {
                if let Some(emf_data) = strip_emf_wrapper(&decompressed) {
                    let name = unique_filename(&format!("{}{}.emf", prefix, stem), used_names);
                    if std::fs::write(output_dir.join(&name), emf_data).is_ok() {
                        return true;
                    }
                }
                // Not EMF either – save decompressed bytes with original stem name.
                let name = unique_filename(&format!("{}{}", prefix, raw_name), used_names);
                if std::fs::write(output_dir.join(&name), &decompressed).is_ok() {
                    return true;
                }
            } else {
                // Too large for RAM – detect EMF from prefix, stream to disk with
                // an appropriate extension.
                let is_emf = gunzip_prefix(bytes)
                    .as_deref()
                    .and_then(strip_emf_wrapper)
                    .is_some();
                let dest_name = if is_emf {
                    format!("{}{}.emf", prefix, stem)
                } else {
                    format!("{}{}", prefix, raw_name)
                };
                let name = unique_filename(&dest_name, used_names);
                if gunzip_to_file(bytes, &output_dir.join(&name)) {
                    return true;
                }
            }
        }
    }

    // ── 6: last resort – save original bytes ─────────────────────────────────
    let orig = format!("{}{}", prefix, raw_name);
    let filename = unique_filename(&orig, used_names);
    std::fs::write(output_dir.join(&filename), bytes).is_ok()
}

/// Convert an `AddressRef` (which doesn't implement Display) to a string.
fn address_ref_to_string(addr: &eml_codec::imf::address::AddressRef<'_>) -> String {
    use eml_codec::imf::address::AddressRef;
    match addr {
        AddressRef::Single(mailbox) => mailbox.to_string(),
        AddressRef::Many(group) => group
            .participants
            .iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Parse EML bytes with eml-codec and render to PDF bytes.
/// Returns None if parsing fails. Note: eml-codec does not decode
/// transfer-encodings (quoted-printable, base64), so body text may appear
/// encoded for non-trivial messages.
fn eml_bytes_to_pdf(bytes: &[u8]) -> Option<Vec<u8>> {
    let (_, email) = eml_codec::parse_message(bytes).ok()?;

    let subject = email
        .imf
        .subject
        .as_ref()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let date = email
        .imf
        .date
        .map(|d| d.to_rfc2822())
        .unwrap_or_else(|| "(unknown date)".to_string());
    let from = email
        .imf
        .from
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let to = email
        .imf
        .to
        .iter()
        .map(address_ref_to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let body = eml_collect_body(&email.child);

    Some(crate::pdf_writer::render_eml_to_pdf(
        &subject, &date, &from, &to, &body,
    ))
}

/// Recursively extract the most-readable text body from a MIME part tree.
/// For `multipart/alternative` we prefer `text/plain` over `text/html`.
/// For other multipart types we join all text children.
fn eml_collect_body(part: &eml_codec::part::AnyPart<'_>) -> String {
    use eml_codec::part::AnyPart;

    match part {
        AnyPart::Txt(text) => {
            let sub = text
                .mime
                .fields
                .ctype
                .as_ref()
                .and_then(|ct| std::str::from_utf8(ct.sub).ok())
                .unwrap_or("plain")
                .to_lowercase();
            let raw = std::str::from_utf8(text.body).unwrap_or("").to_string();
            if sub == "html" {
                strip_html(&raw)
            } else {
                raw
            }
        }
        AnyPart::Mult(mp) => {
            let sub = mp
                .mime
                .fields
                .ctype
                .as_ref()
                .and_then(|ct| std::str::from_utf8(ct.sub).ok())
                .unwrap_or("")
                .to_lowercase();

            if sub == "alternative" {
                // Prefer plain-text alternative; fall back to the first child.
                if let Some(plain) = mp.children.iter().find(|c| eml_is_plain_text(c)) {
                    return eml_collect_body(plain);
                }
                mp.children
                    .first()
                    .map(eml_collect_body)
                    .unwrap_or_default()
            } else {
                mp.children
                    .iter()
                    .map(eml_collect_body)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        AnyPart::Msg(msg) => eml_collect_body(&msg.child),
        AnyPart::Bin(_) => String::new(),
    }
}

fn eml_is_plain_text(part: &eml_codec::part::AnyPart<'_>) -> bool {
    if let eml_codec::part::AnyPart::Txt(text) = part {
        let sub = text
            .mime
            .fields
            .ctype
            .as_ref()
            .and_then(|ct| std::str::from_utf8(ct.sub).ok())
            .unwrap_or("plain")
            .to_lowercase();
        sub == "plain"
    } else {
        false
    }
}

/// Detects and strips the 32-byte Outlook/OLE EMF wrapper.
/// Returns `Some(&[u8])` containing the raw EMF stream if detected,
/// or `None` if the data is not a wrapped EMF.
fn strip_emf_wrapper(data: &[u8]) -> Option<&[u8]> {
    const WRAPPER_LEN: usize = 32;
    if data.len() < WRAPPER_LEN + 8 {
        return None;
    }
    if &data[WRAPPER_LEN..WRAPPER_LEN + 4] != b"EMF\0" {
        return None;
    }
    let record_type = u32::from_le_bytes([
        data[WRAPPER_LEN + 4],
        data[WRAPPER_LEN + 5],
        data[WRAPPER_LEN + 6],
        data[WRAPPER_LEN + 7],
    ]);
    if record_type != 1 {
        return None;
    }
    Some(&data[WRAPPER_LEN..])
}

/// Returns true if the bytes start with the gzip magic number.
fn is_gzip_magic(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
}

/// Decompress only the first ~512 bytes of a gzip stream for heuristic checks.
fn gunzip_prefix(bytes: &[u8]) -> Option<Vec<u8>> {
    use flate2::read::MultiGzDecoder;
    let mut dec = MultiGzDecoder::new(bytes);
    let mut buf = vec![0u8; 512];
    let n = dec.read(&mut buf).ok()?;
    if n == 0 { None } else { buf.truncate(n); Some(buf) }
}

/// Decompress a gzip stream into memory, but only up to `MAX_EML_BYTES`.
/// Returns `None` if the stream is invalid or exceeds the limit.
fn try_gunzip_limited(bytes: &[u8]) -> Option<Vec<u8>> {
    use flate2::read::MultiGzDecoder;
    let mut out = Vec::new();
    // Read one extra byte so we can detect "exceeds limit" vs "reached EOF".
    MultiGzDecoder::new(bytes)
        .take(MAX_EML_BYTES + 1)
        .read_to_end(&mut out)
        .ok()?;
    if out.is_empty() || out.len() as u64 > MAX_EML_BYTES {
        None
    } else {
        Some(out)
    }
}

/// Stream a gzip archive to a file without buffering in RAM.
fn gunzip_to_file(bytes: &[u8], dest: &Path) -> bool {
    use flate2::read::MultiGzDecoder;
    let file = match std::fs::File::create(dest) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut dec = MultiGzDecoder::new(bytes);
    std::io::copy(&mut dec, &mut std::io::BufWriter::new(file)).is_ok()
}

/// Return true if the byte slice is a valid zip archive that contains at
/// least one `.eml` entry (does not extract anything).
fn zip_has_eml(bytes: &[u8]) -> bool {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = match zip::ZipArchive::new(cursor) {
        Ok(a) => a,
        Err(_) => return false,
    };
    (0..archive.len()).any(|i| {
        archive
            .by_index(i)
            .ok()
            .map(|f| f.name().to_lowercase().ends_with(".eml"))
            .unwrap_or(false)
    })
}

/// Extract the first `.eml` entry from a zip archive into memory.
/// Returns `None` if no `.eml` entry exists or the entry exceeds `MAX_EML_BYTES`.
fn try_zip_find_eml(bytes: &[u8]) -> Option<Vec<u8>> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    for i in 0..archive.len() {
        let file = match archive.by_index(i) {
            Ok(f) => f,
            Err(_) => continue,
        };
        if !file.name().to_lowercase().ends_with(".eml") {
            continue;
        }
        if file.size() > MAX_EML_BYTES {
            return None; // signal: entry exists but is too large
        }
        let mut buf = Vec::new();
        if file.take(MAX_EML_BYTES).read_to_end(&mut buf).is_ok() && !buf.is_empty() {
            return Some(buf);
        }
    }
    None
}

/// Stream the first `.eml` entry in a zip archive directly to a file.
fn zip_stream_eml_to_file(bytes: &[u8], dest: &Path) -> bool {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = match zip::ZipArchive::new(cursor) {
        Ok(a) => a,
        Err(_) => return false,
    };
    for i in 0..archive.len() {
        let mut file = match archive.by_index(i) {
            Ok(f) => f,
            Err(_) => continue,
        };
        if file.name().to_lowercase().ends_with(".eml") {
            if let Ok(out) = std::fs::File::create(dest) {
                return std::io::copy(&mut file, &mut std::io::BufWriter::new(out)).is_ok();
            }
        }
    }
    false
}

/// Heuristic: does this byte slice look like an RFC-5322 email message?
fn looks_like_eml(data: &[u8]) -> bool {
    let prefix = std::str::from_utf8(&data[..data.len().min(512)]).unwrap_or("");
    prefix.contains("From:") || prefix.contains("Date:") || prefix.contains("MIME-Version:")
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── strip_html ───────────────────────────────────────────────────────────

    #[test]
    fn strip_html_removes_tags() {
        assert_eq!(strip_html("<p>Hello</p>"), "Hello");
    }

    #[test]
    fn strip_html_plain_text_unchanged() {
        assert_eq!(strip_html("Hello World"), "Hello World");
    }

    #[test]
    fn strip_html_decodes_entities() {
        assert_eq!(strip_html("&amp;&lt;&gt;&nbsp;&quot;&#39;"), "&<> \"'");
    }

    #[test]
    fn strip_html_nested_tags() {
        assert_eq!(strip_html("<div><b>bold</b> text</div>"), "bold text");
    }

    #[test]
    fn strip_html_empty_string() {
        assert_eq!(strip_html(""), "");
    }

    #[test]
    fn strip_html_only_tags() {
        assert_eq!(strip_html("<br/><hr/>"), "");
    }

    // ── filetime_to_datetime ─────────────────────────────────────────────────

    #[test]
    fn filetime_unix_epoch() {
        // FILETIME for 1970-01-01 00:00:00 UTC = 116444736000000000
        let dt = filetime_to_datetime(116_444_736_000_000_000).unwrap();
        assert_eq!(dt.timestamp(), 0);
    }

    #[test]
    fn filetime_y2k() {
        // 2000-01-01 00:00:00 UTC: unix ts = 946684800
        // FILETIME = (946684800 + 11644473600) * 10_000_000
        let ft = (946_684_800i64 + 11_644_473_600) * 10_000_000;
        let dt = filetime_to_datetime(ft).unwrap();
        assert_eq!(dt.timestamp(), 946_684_800);
    }

    // ── unique_filename ──────────────────────────────────────────────────────

    #[test]
    fn unique_filename_new_name() {
        let mut used = HashSet::new();
        assert_eq!(unique_filename("photo.jpg", &mut used), "photo.jpg");
    }

    #[test]
    fn unique_filename_duplicate_adds_counter() {
        let mut used = HashSet::new();
        unique_filename("photo.jpg", &mut used);
        assert_eq!(unique_filename("photo.jpg", &mut used), "photo_1.jpg");
    }

    #[test]
    fn unique_filename_multiple_duplicates() {
        let mut used = HashSet::new();
        unique_filename("file.txt", &mut used);
        unique_filename("file.txt", &mut used); // → file_1.txt
        assert_eq!(unique_filename("file.txt", &mut used), "file_2.txt");
    }

    #[test]
    fn unique_filename_no_extension() {
        let mut used = HashSet::new();
        unique_filename("readme", &mut used);
        assert_eq!(unique_filename("readme", &mut used), "readme_1");
    }

    #[test]
    fn unique_filename_sanitizes_slashes() {
        let mut used = HashSet::new();
        assert_eq!(unique_filename("a/b\\c", &mut used), "a_b_c");
    }

    #[test]
    fn unique_filename_empty_becomes_default() {
        let mut used = HashSet::new();
        assert_eq!(unique_filename("", &mut used), "attachment.bin");
    }

    #[test]
    fn unique_filename_whitespace_only_becomes_default() {
        let mut used = HashSet::new();
        assert_eq!(unique_filename("   ", &mut used), "attachment.bin");
    }
}
