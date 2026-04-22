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

    println!("Writing PDF: {}", output_path.display());
    pdf_writer::write_pdf(&threads, &output_path, args.showdetails)
        .with_context(|| format!("Failed to write {}", output_path.display()))?;

    println!("Done.");

    if let Some(att_dir) = &args.attachments {
        println!("Extracting attachments to: {}", att_dir.display());
        let count = pst_reader::save_attachments(&args.pst, att_dir)
            .with_context(|| format!("Failed to extract attachments to {}", att_dir.display()))?;
        println!("Saved {} attachment(s).", count);
    }

    Ok(())
}
