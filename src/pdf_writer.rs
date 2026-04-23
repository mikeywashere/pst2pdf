use std::path::Path;

use anyhow::{Context, Result};
use chrono::DateTime;
use printpdf::{
    BuiltinFont, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt, Rect,
    TextItem,
};

use crate::models::ConversationThread;

const PAGE_WIDTH_MM: f32 = 210.0;
const PAGE_HEIGHT_MM: f32 = 297.0;
const MARGIN_MM: f32 = 15.0;
const FONT_SIZE_PT: f32 = 10.0;
const HEADER_SIZE_PT: f32 = 12.0;
const LINE_HEIGHT_PT: f32 = 14.0;

// Approximate characters per line at 10pt Helvetica on A4 with 15mm margins
const MAX_CHARS_PER_LINE: usize = 95;

fn make_page_rect() -> Rect {
    Rect {
        x: Pt(0.0),
        y: Pt(0.0),
        width: Pt(mm_to_pt(PAGE_WIDTH_MM)),
        height: Pt(mm_to_pt(PAGE_HEIGHT_MM)),
        mode: None,
        winding_order: None,
    }
}

fn mm_to_pt(mm: f32) -> f32 {
    mm * 72.0 / 25.4
}

fn pt_to_mm(pt: f32) -> f32 {
    pt * 25.4 / 72.0
}

// Strip non-Latin1 characters since built-in PDF fonts use Windows-1252
fn sanitize_text(s: &str) -> String {
    s.chars()
        .map(|c| if c as u32 > 255 || (c as u32 > 127 && (c as u32) < 160) { '?' } else { c })
        .collect()
}

fn is_exchange_dn(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("/O=") || t.starts_with("/o=")
}

// Return the display string for an address, hiding Exchange DNs unless show_details is true.
// Returns None if there is nothing worth displaying.
fn clean_address(raw: &str, show_details: bool) -> Option<String> {
    if show_details {
        return Some(raw.to_string());
    }
    // Handle "Display Name <addr>" format
    if let (Some(lt), true) = (raw.find('<'), raw.ends_with('>')) {
        let name = raw[..lt].trim();
        let addr = &raw[lt + 1..raw.len() - 1];
        if is_exchange_dn(addr) {
            return if name.is_empty() { None } else { Some(name.to_string()) };
        }
    } else if is_exchange_dn(raw) {
        return None;
    }
    Some(raw.to_string())
}

fn word_wrap(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.lines() {
        if para.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in para.split_whitespace() {
            if current.is_empty() {
                current.push_str(word);
            } else if current.len() + 1 + word.len() <= max_chars {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current.clone());
                current = word.to_string();
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}

struct PageWriter {
    doc: PdfDocument,
    ops: Vec<Op>,
    cursor_y_mm: f32,
    in_text: bool,
}

impl PageWriter {
    fn new(title: &str) -> Self {
        let doc = PdfDocument::new(title);
        PageWriter {
            doc,
            ops: Vec::new(),
            cursor_y_mm: PAGE_HEIGHT_MM - MARGIN_MM,
            in_text: false,
        }
    }

    fn start_text_section(&mut self, font: &PdfFontHandle, size: f32) {
        self.ops.push(Op::StartTextSection);
        self.ops.push(Op::SetFont {
            font: font.clone(),
            size: Pt(size),
        });
        self.ops.push(Op::SetLineHeight {
            lh: Pt(LINE_HEIGHT_PT),
        });
        self.ops.push(Op::SetTextCursor {
            pos: Point::new(Mm(MARGIN_MM), Mm(self.cursor_y_mm)),
        });
        self.in_text = true;
    }

    fn end_text_section(&mut self) {
        if self.in_text {
            self.ops.push(Op::EndTextSection);
            self.in_text = false;
        }
    }

    fn flush_page(&mut self) {
        self.end_text_section();
        let rect = make_page_rect();
        let page = PdfPage {
            media_box: rect.clone(),
            trim_box: rect.clone(),
            crop_box: rect,
            ops: std::mem::take(&mut self.ops),
        };
        self.doc.pages.push(page);
        self.cursor_y_mm = PAGE_HEIGHT_MM - MARGIN_MM;
    }

    fn need_new_page(&self) -> bool {
        self.cursor_y_mm < MARGIN_MM + pt_to_mm(LINE_HEIGHT_PT * 2.0)
    }

    fn new_page(&mut self, font: &PdfFontHandle, size: f32) {
        self.flush_page();
        self.start_text_section(font, size);
    }

    fn write_line(&mut self, text: &str, font: &PdfFontHandle, size: f32) {
        if !self.in_text {
            self.start_text_section(font, size);
        }
        let safe = sanitize_text(text);
        self.ops.push(Op::ShowText {
            items: vec![TextItem::Text(safe)],
        });
        self.ops.push(Op::AddLineBreak);
        self.cursor_y_mm -= pt_to_mm(LINE_HEIGHT_PT);

        if self.need_new_page() {
            self.new_page(font, size);
        }
    }

    fn write_blank_line(&mut self, font: &PdfFontHandle, size: f32) {
        self.write_line("", font, size);
    }

    fn write_wrapped(&mut self, text: &str, font: &PdfFontHandle, size: f32) {
        let lines = word_wrap(text, MAX_CHARS_PER_LINE);
        for line in lines {
            self.write_line(&line, font, size);
        }
    }

    fn finalize(mut self) -> PdfDocument {
        if !self.ops.is_empty() || self.doc.pages.is_empty() {
            self.flush_page();
        }
        self.doc
    }
}

fn render_messages_to_writer(
    writer: &mut PageWriter,
    messages: &[crate::models::EmailMessage],
    normal_font: &PdfFontHandle,
    bold_font: &PdfFontHandle,
    show_details: bool,
) {
    for msg in messages {
        writer.write_blank_line(normal_font, FONT_SIZE_PT);

        let date_str = msg
            .date
            .map(|d: DateTime<_>| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "(unknown date)".to_string());

        let from_str = {
            let addr_clean = if is_exchange_dn(&msg.from_address) && !show_details {
                String::new()
            } else {
                msg.from_address.clone()
            };
            if addr_clean.is_empty() {
                msg.from_name.clone()
            } else if msg.from_name.is_empty() {
                addr_clean
            } else {
                format!("{} <{}>", msg.from_name, addr_clean)
            }
        };

        let to_parts: Vec<String> = msg
            .to_recipients
            .iter()
            .filter_map(|r| clean_address(r, show_details))
            .collect();
        let to_str = if to_parts.is_empty() {
            "(unknown)".to_string()
        } else {
            to_parts.join(", ")
        };

        writer.write_line(&format!("Date:    {}", date_str), bold_font, FONT_SIZE_PT);
        writer.write_line(&format!("From:    {}", from_str), bold_font, FONT_SIZE_PT);
        writer.write_wrapped(&format!("To:      {}", to_str), bold_font, FONT_SIZE_PT);
        writer.write_line(&format!("Subject: {}", msg.subject), bold_font, FONT_SIZE_PT);
        writer.write_line(&"-".repeat(60), normal_font, FONT_SIZE_PT);

        if msg.body.is_empty() {
            writer.write_line("(no body)", normal_font, FONT_SIZE_PT);
        } else {
            writer.write_wrapped(&msg.body, normal_font, FONT_SIZE_PT);
        }
    }
}

/// Render a parsed EML file into a PDF and return the raw bytes.
/// Headers are printed in bold; body in regular font.
pub fn render_eml_to_pdf(subject: &str, date: &str, from: &str, to: &str, body: &str) -> Vec<u8> {
    let title = if subject.is_empty() { "email" } else { subject };
    let mut writer = PageWriter::new(title);
    let normal_font = PdfFontHandle::Builtin(BuiltinFont::Helvetica);
    let bold_font = PdfFontHandle::Builtin(BuiltinFont::HelveticaBold);

    writer.write_line(&format!("Date:    {}", date), &bold_font, FONT_SIZE_PT);
    writer.write_wrapped(&format!("From:    {}", from), &bold_font, FONT_SIZE_PT);
    writer.write_wrapped(&format!("To:      {}", to), &bold_font, FONT_SIZE_PT);
    writer.write_wrapped(&format!("Subject: {}", subject), &bold_font, FONT_SIZE_PT);
    writer.write_line(&"-".repeat(60), &normal_font, FONT_SIZE_PT);

    if body.is_empty() {
        writer.write_line("(no body)", &normal_font, FONT_SIZE_PT);
    } else {
        writer.write_wrapped(body, &normal_font, FONT_SIZE_PT);
    }

    let doc = writer.finalize();
    doc.save(&PdfSaveOptions::default(), &mut Vec::new())
}

pub fn write_pdf(threads: &[ConversationThread], output_path: &Path, show_details: bool) -> Result<()> {
    let title = output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("pst2pdf");

    let mut writer = PageWriter::new(title);
    let normal_font = PdfFontHandle::Builtin(BuiltinFont::Helvetica);
    let bold_font = PdfFontHandle::Builtin(BuiltinFont::HelveticaBold);

    for (i, thread) in threads.iter().enumerate() {
        if i > 0 {
            writer.write_blank_line(&normal_font, FONT_SIZE_PT);
        }

        // Thread header
        let header = format!(
            "Thread: {} ({} message{})",
            thread.display_subject,
            thread.messages.len(),
            if thread.messages.len() == 1 { "" } else { "s" }
        );
        writer.write_line(&header, &bold_font, HEADER_SIZE_PT);
        writer.write_line(
            &"=".repeat(header.len().min(MAX_CHARS_PER_LINE)),
            &normal_font,
            FONT_SIZE_PT,
        );

        render_messages_to_writer(&mut writer, &thread.messages, &normal_font, &bold_font, show_details);
    }

    let doc = writer.finalize();
    let bytes = doc.save(&PdfSaveOptions::default(), &mut Vec::new());
    std::fs::write(output_path, &bytes)?;
    Ok(())
}

/// Write one PDF file per conversation thread.
/// Files are named `<stem>-00001.pdf`, `<stem>-00002.pdf`, etc., written into
/// `output_dir`. Each PDF contains only the messages for that thread (no bold
/// "Thread:" header block).
pub fn write_conversation_pdfs(
    threads: &[ConversationThread],
    output_dir: &Path,
    stem: &str,
    show_details: bool,
) -> Result<()> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create directory: {}", output_dir.display()))?;

    let normal_font = PdfFontHandle::Builtin(BuiltinFont::Helvetica);
    let bold_font = PdfFontHandle::Builtin(BuiltinFont::HelveticaBold);

    for (i, thread) in threads.iter().enumerate() {
        let filename = format!("{}-{:05}.pdf", stem, i + 1);
        let path = output_dir.join(&filename);

        let mut writer = PageWriter::new(&thread.display_subject);
        render_messages_to_writer(&mut writer, &thread.messages, &normal_font, &bold_font, show_details);

        let doc = writer.finalize();
        let bytes = doc.save(&PdfSaveOptions::default(), &mut Vec::new());
        std::fs::write(&path, &bytes)
            .with_context(|| format!("Failed to write {}", path.display()))?;
    }
    Ok(())
}

// ── Plain-text output ────────────────────────────────────────────────────────

fn render_messages_to_text(
    out: &mut String,
    messages: &[crate::models::EmailMessage],
    show_details: bool,
) {
    for msg in messages {
        out.push('\n');

        let date_str = msg
            .date
            .map(|d: DateTime<_>| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "(unknown date)".to_string());

        let from_str = {
            let addr_clean = if is_exchange_dn(&msg.from_address) && !show_details {
                String::new()
            } else {
                msg.from_address.clone()
            };
            if addr_clean.is_empty() {
                msg.from_name.clone()
            } else if msg.from_name.is_empty() {
                addr_clean
            } else {
                format!("{} <{}>", msg.from_name, addr_clean)
            }
        };

        let to_parts: Vec<String> = msg
            .to_recipients
            .iter()
            .filter_map(|r| clean_address(r, show_details))
            .collect();
        let to_str = if to_parts.is_empty() {
            "(unknown)".to_string()
        } else {
            to_parts.join(", ")
        };

        out.push_str(&format!("Date:    {}\n", date_str));
        out.push_str(&format!("From:    {}\n", from_str));
        out.push_str(&format!("To:      {}\n", to_str));
        out.push_str(&format!("Subject: {}\n", msg.subject));
        out.push_str(&"-".repeat(60));
        out.push('\n');

        if msg.body.is_empty() {
            out.push_str("(no body)\n");
        } else {
            out.push_str(&msg.body);
            if !msg.body.ends_with('\n') {
                out.push('\n');
            }
        }
    }
}

/// Write all threads to a single UTF-8 plain-text file.
pub fn write_text(threads: &[ConversationThread], output_path: &Path, show_details: bool) -> Result<()> {
    let mut out = String::new();

    for (i, thread) in threads.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let header = format!(
            "Thread: {} ({} message{})",
            thread.display_subject,
            thread.messages.len(),
            if thread.messages.len() == 1 { "" } else { "s" }
        );
        out.push_str(&header);
        out.push('\n');
        out.push_str(&"=".repeat(header.len().min(80)));
        out.push('\n');

        render_messages_to_text(&mut out, &thread.messages, show_details);
    }

    std::fs::write(output_path, out.as_bytes())
        .with_context(|| format!("Failed to write {}", output_path.display()))?;
    Ok(())
}

/// Write one UTF-8 plain-text file per conversation thread.
/// Files are named `<stem>-00001.txt`, `<stem>-00002.txt`, etc.
pub fn write_conversation_texts(
    threads: &[ConversationThread],
    output_dir: &Path,
    stem: &str,
    show_details: bool,
) -> Result<()> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create directory: {}", output_dir.display()))?;

    for (i, thread) in threads.iter().enumerate() {
        let filename = format!("{}-{:05}.txt", stem, i + 1);
        let path = output_dir.join(&filename);

        let mut out = String::new();
        render_messages_to_text(&mut out, &thread.messages, show_details);

        std::fs::write(&path, out.as_bytes())
            .with_context(|| format!("Failed to write {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── mm_to_pt / pt_to_mm ──────────────────────────────────────────────────

    #[test]
    fn mm_to_pt_a4_width() {
        // 210mm = 595.28pt (standard A4)
        let pt = mm_to_pt(210.0);
        assert!((pt - 595.28).abs() < 0.1, "got {}", pt);
    }

    #[test]
    fn pt_to_mm_roundtrip() {
        let mm = 100.0f32;
        assert!((pt_to_mm(mm_to_pt(mm)) - mm).abs() < 0.001);
    }

    // ── sanitize_text ────────────────────────────────────────────────────────

    #[test]
    fn sanitize_ascii_unchanged() {
        assert_eq!(sanitize_text("Hello, World!"), "Hello, World!");
    }

    #[test]
    fn sanitize_latin1_supplement_passes() {
        // é (0xE9) is valid Latin-1
        assert_eq!(sanitize_text("café"), "café");
    }

    #[test]
    fn sanitize_unicode_beyond_latin1_replaced() {
        // € is U+20AC, beyond Latin-1
        let result = sanitize_text("price: €100");
        assert_eq!(result, "price: ?100");
    }

    #[test]
    fn sanitize_c1_control_range_replaced() {
        // U+0080–U+009F are C1 controls, not printable in Windows-1252
        let s: String = "\u{0082}".to_string();
        assert_eq!(sanitize_text(&s), "?");
    }

    #[test]
    fn sanitize_emoji_replaced() {
        assert_eq!(sanitize_text("hi 😀"), "hi ?");
    }

    // ── is_exchange_dn ───────────────────────────────────────────────────────

    #[test]
    fn exchange_dn_detected_uppercase() {
        assert!(is_exchange_dn("/O=CORP/OU=EXCHANGE/CN=user"));
    }

    #[test]
    fn exchange_dn_detected_lowercase() {
        assert!(is_exchange_dn("/o=corp/ou=exchange/cn=user"));
    }

    #[test]
    fn exchange_dn_with_leading_whitespace() {
        assert!(is_exchange_dn("  /O=CORP/CN=user"));
    }

    #[test]
    fn normal_email_not_dn() {
        assert!(!is_exchange_dn("user@example.com"));
    }

    #[test]
    fn empty_string_not_dn() {
        assert!(!is_exchange_dn(""));
    }

    // ── clean_address ────────────────────────────────────────────────────────

    #[test]
    fn clean_normal_email_passthrough() {
        assert_eq!(
            clean_address("user@example.com", false),
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn clean_exchange_dn_hidden() {
        assert_eq!(clean_address("/O=CORP/CN=user", false), None);
    }

    #[test]
    fn clean_exchange_dn_shown_with_details() {
        assert_eq!(
            clean_address("/O=CORP/CN=user", true),
            Some("/O=CORP/CN=user".to_string())
        );
    }

    #[test]
    fn clean_display_name_with_exchange_dn_shows_name() {
        assert_eq!(
            clean_address("John Smith </O=CORP/CN=jsmith>", false),
            Some("John Smith".to_string())
        );
    }

    #[test]
    fn clean_no_name_with_exchange_dn_returns_none() {
        assert_eq!(clean_address("</O=CORP/CN=jsmith>", false), None);
    }

    #[test]
    fn clean_display_name_with_exchange_dn_shown_with_details() {
        assert_eq!(
            clean_address("John Smith </O=CORP/CN=jsmith>", true),
            Some("John Smith </O=CORP/CN=jsmith>".to_string())
        );
    }

    #[test]
    fn clean_normal_name_and_email_passthrough() {
        assert_eq!(
            clean_address("John Smith <john@example.com>", false),
            Some("John Smith <john@example.com>".to_string())
        );
    }

    // ── word_wrap ────────────────────────────────────────────────────────────

    #[test]
    fn wrap_short_text_single_line() {
        let lines = word_wrap("Hello world", 80);
        assert_eq!(lines, vec!["Hello world"]);
    }

    #[test]
    fn wrap_long_text_breaks() {
        let text = "one two three four five";
        let lines = word_wrap(text, 12);
        // "one two" = 7, "three four" = 10, "five" = 4
        assert!(lines.len() > 1);
        // Reassembled text should match original words
        let rejoined = lines.join(" ");
        assert_eq!(rejoined, text);
    }

    #[test]
    fn wrap_preserves_empty_lines() {
        let lines = word_wrap("para one\n\npara two", 80);
        assert!(lines.contains(&String::new()), "empty line should be preserved");
    }

    #[test]
    fn wrap_empty_string() {
        let lines = word_wrap("", 80);
        assert!(lines.is_empty());
    }

    #[test]
    fn wrap_single_word_longer_than_limit() {
        // A word longer than max_chars should still appear on its own line
        let lines = word_wrap("superlongword", 5);
        assert_eq!(lines, vec!["superlongword"]);
    }
}
