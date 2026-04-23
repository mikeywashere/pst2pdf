mod models;
mod pdf_writer;
mod pst_reader;
mod thread_grouper;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

fn is_dir_like(path: &PathBuf) -> bool {
    let s = path.to_string_lossy();
    path.is_dir() || s.ends_with('\\') || s.ends_with('/')
}

#[derive(Parser)]
#[command(name = "pst2pdf")]
#[command(about = "Convert an Outlook PST archive to PDF and/or text")]
#[command(long_about = "Convert an Outlook PST archive to PDF and/or plain-text files.\n\
\n\
Examples:\n\
  pst2pdf --pst archive.pst\n\
  pst2pdf --pst archive.pst --as text\n\
  pst2pdf --pst archive.pst --as pdf,text\n\
  pst2pdf --pst archive.pst --conversations --output ./out --as text\n\
  pst2pdf --pst archive.pst --attachments ./attachments")]
#[command(arg_required_else_help = true)]
struct Args {
    /// Path to the PST file to convert
    #[arg(long, value_name = "FILE")]
    pst: PathBuf,

    /// Output PDF file or folder.
    /// If a folder, the PST filename is used as the base name inside it.
    /// With --conversations this must be a folder; numbered files are written there.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Show internal Exchange addresses (e.g. /O=.../CN=...) in From/To fields
    #[arg(long)]
    showdetails: bool,

    /// Folder to export attachments into (optional)
    #[arg(long, value_name = "DIR")]
    attachments: Option<PathBuf>,

    /// Write one file per conversation thread instead of a single combined file.
    /// Files are named <output>-00001.<ext>, <output>-00002.<ext>, etc.
    #[arg(long)]
    conversations: bool,

    /// Output format(s): pdf, text, or both (comma-separated). Default: pdf.
    ///
    /// Examples: --as pdf  --as text  --as pdf,text
    #[arg(long = "as", value_name = "FORMAT", value_delimiter = ',', default_value = "pdf")]
    output_format: Vec<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Validate --as values
    let want_pdf = args.output_format.iter().any(|f| f.eq_ignore_ascii_case("pdf"));
    let want_text = args.output_format.iter().any(|f| f.eq_ignore_ascii_case("text"));
    if !want_pdf && !want_text {
        eprintln!("error: --as accepts 'pdf' and/or 'text' (e.g. --as pdf,text)");
        std::process::exit(1);
    }

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
        let output_dir = args.output.clone().unwrap_or_else(|| pst_dir.clone());

        if want_pdf {
            println!("Writing {} conversation PDFs to: {}", threads.len(), output_dir.display());
            pdf_writer::write_conversation_pdfs(&threads, &output_dir, &pst_stem, args.showdetails)
                .with_context(|| format!("Failed to write conversation PDFs to {}", output_dir.display()))?;
        }

        if want_text {
            println!("Writing {} conversation text files to: {}", threads.len(), output_dir.display());
            pdf_writer::write_conversation_texts(&threads, &output_dir, &pst_stem, args.showdetails)
                .with_context(|| format!("Failed to write conversation text files to {}", output_dir.display()))?;
        }

        if let Some(att_dir) = &args.attachments {
            println!("Extracting attachments to: {}", att_dir.display());
            let count = pst_reader::save_attachments_for_threads(&args.pst, att_dir, &threads, &pst_stem)
                .with_context(|| format!("Failed to extract attachments to {}", att_dir.display()))?;
            println!("Saved {} attachment(s).", count);
        }
    } else {
        // Resolve base output path (without extension).
        let base_path = match args.output {
            Some(ref p) if is_dir_like(p) => p.join(&pst_stem),
            Some(ref p) => p
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(|parent| {
                    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(&pst_stem);
                    parent.join(stem)
                })
                .unwrap_or_else(|| {
                    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(&pst_stem);
                    PathBuf::from(stem)
                }),
            None => pst_dir.join(&pst_stem),
        };

        if want_pdf {
            let pdf_path = base_path.with_extension("pdf");
            println!("Writing PDF: {}", pdf_path.display());
            pdf_writer::write_pdf(&threads, &pdf_path, args.showdetails)
                .with_context(|| format!("Failed to write {}", pdf_path.display()))?;
        }

        if want_text {
            let txt_path = base_path.with_extension("txt");
            println!("Writing text: {}", txt_path.display());
            pdf_writer::write_text(&threads, &txt_path, args.showdetails)
                .with_context(|| format!("Failed to write {}", txt_path.display()))?;
        }

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
