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

    /// Output PDF file path (defaults to <pst-name>.pdf)
    #[arg(long, value_name = "FILE")]
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

    let output_path = args.output.unwrap_or_else(|| {
        let mut p = args.pst.clone();
        p.set_extension("pdf");
        p
    });

    println!("Reading PST: {}", args.pst.display());
    let messages = pst_reader::read_messages(&args.pst)
        .with_context(|| format!("Failed to read {}", args.pst.display()))?;

    println!("Found {} messages", messages.len());
    let threads = thread_grouper::group_by_thread(messages);
    println!("Grouped into {} conversation threads", threads.len());

    if args.conversations {
        let stem = output_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("pst2pdf")
            .to_string();
        let parent = output_path.parent().unwrap_or(std::path::Path::new("."));
        if let Some(p) = parent.to_str().filter(|s| !s.is_empty()) {
            println!("Writing {} conversation PDFs to: {}", threads.len(), p);
        }
        pdf_writer::write_conversation_pdfs(&threads, &output_path, args.showdetails)
            .with_context(|| format!("Failed to write conversation PDFs"))?;

        if let Some(att_dir) = &args.attachments {
            println!("Extracting attachments to: {}", att_dir.display());
            let count = pst_reader::save_attachments_for_threads(&args.pst, att_dir, &threads, &stem)
                .with_context(|| format!("Failed to extract attachments to {}", att_dir.display()))?;
            println!("Saved {} attachment(s).", count);
        }
    } else {
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
