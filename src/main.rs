mod models;
mod pdf_writer;
mod pst_reader;
mod thread_grouper;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

#[derive(Parser)]
#[command(name = "pst2pdf")]
#[command(about = "Convert an Outlook PST archive to a PDF file")]
#[command(arg_required_else_help = true)]
struct Args {
    /// Path to the PST file to convert
    #[arg(long, value_name = "FILE")]
    pst: PathBuf,

    /// Output PDF file or folder.
    /// If a folder, the PST filename is used as the PDF name inside it.
    /// With --conversations this must be a folder; numbered PDFs are written there.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Show internal Exchange addresses (e.g. /O=.../CN=...) in From/To fields
    #[arg(long)]
    showdetails: bool,

    /// Folder to export attachments into (optional)
    #[arg(long, value_name = "DIR")]
    attachments: Option<PathBuf>,

    /// Write one PDF per conversation thread instead of a single combined PDF.
    /// Files are named <output>-00001.pdf, <output>-00002.pdf, etc.
    #[arg(long)]
    conversations: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Derive the stem (base filename without extension) from the PST file.
    let pst_stem = args.pst
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
        .to_string();

    // Default output directory is the PST file's own directory.
    let pst_dir = args.pst
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    println!("Reading PST: {}", args.pst.display());
    let messages = pst_reader::read_messages(&args.pst)
        .with_context(|| format!("Failed to read {}", args.pst.display()))?;

    println!("Found {} messages", messages.len());
    let threads = thread_grouper::group_by_thread(messages);
    println!("Grouped into {} conversation threads", threads.len());

    if args.conversations {
        // --conversations: --output is always a directory.
        let output_dir = args.output.clone().unwrap_or_else(|| pst_dir.clone());

        println!("Writing {} conversation PDFs to: {}", threads.len(), output_dir.display());
        pdf_writer::write_conversation_pdfs(&threads, &output_dir, &pst_stem, args.showdetails)
            .with_context(|| format!("Failed to write conversation PDFs to {}", output_dir.display()))?;

        if let Some(att_dir) = &args.attachments {
            println!("Extracting attachments to: {}", att_dir.display());
            let count = pst_reader::save_attachments_for_threads(&args.pst, att_dir, &threads, &pst_stem)
                .with_context(|| format!("Failed to extract attachments to {}", att_dir.display()))?;
            println!("Saved {} attachment(s).", count);
        }
    } else {
        // Normal mode: resolve --output as a file or folder.
        let output_path = match args.output {
            // Existing directory → place <pst_stem>.pdf inside it
            Some(ref p) if p.is_dir() => p.join(format!("{}.pdf", pst_stem)),
            // Explicit file path (or not-yet-existing path) → use as-is
            Some(p) => p,
            // No --output → same directory as the PST file
            None => pst_dir.join(format!("{}.pdf", pst_stem)),
        };

        println!("Writing PDF: {}", output_path.display());
        pdf_writer::write_pdf(&threads, &output_path, args.showdetails)
            .with_context(|| format!("Failed to write {}", output_path.display()))?;

        if let Some(att_dir) = &args.attachments {
            println!("Extracting attachments to: {}", att_dir.display());
            let count = pst_reader::save_attachments(&args.pst, att_dir)
                .with_context(|| format!("Failed to extract attachments to {}", att_dir.display()))?;
            println!("Saved {} attachment(s).", count);
        }
    }

    println!("Done.");
    Ok(())
}
