mod models;
mod pdf_writer;
mod pst_reader;
mod thread_grouper;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};

fn is_dir_like(path: &PathBuf) -> bool {
    let s = path.to_string_lossy();
    path.is_dir() || s.ends_with('\\') || s.ends_with('/')
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }
    Ok(())
}

fn sort_flat_messages(
    mut messages: Vec<models::EmailMessage>,
) -> Vec<models::EmailMessage> {
    messages.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.node_id.cmp(&b.node_id)));
    messages
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
  pst2pdf --pst archive.pst --as none --attachments ./attachments\n\
  pst2pdf --pst archive.pst --verbose\n\
  pst2pdf --pst archive.pst --filter png,eml,txt,msg\n\
  pst2pdf --pst archive.pst --attachments ./attachments --unzip\n\
  pst2pdf --pst archive.pst --by flat --into individual --output ./out\n\
  pst2pdf --pst archive.pst --by conversation --into individual --output ./out --as text\n\
  pst2pdf --pst archive.pst --attachments ./attachments")]
#[command(arg_required_else_help = true)]
struct Args {
    /// Path to the PST file to convert
    #[arg(long, value_name = "FILE")]
    pst: PathBuf,

    /// Output PDF file or folder.
    /// If a folder, the PST filename is used as the base name inside it.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Show internal Exchange addresses (e.g. /O=.../CN=...) in From/To fields
    #[arg(long)]
    showdetails: bool,

    /// Folder to export attachments into (optional)
    #[arg(long, value_name = "DIR")]
    attachments: Option<PathBuf>,

    /// Group output by flat email order or conversation threads.
    #[arg(long, value_enum, default_value_t = OutputBy::Conversation)]
    by: OutputBy,

    /// Write one combined file or one file per email/conversation.
    #[arg(long, value_enum, default_value_t = OutputInto::One)]
    into: OutputInto,

    /// Output format(s): pdf, text, both, or none (comma-separated). Default: pdf.
    ///
    /// Examples: --as pdf  --as text  --as pdf,text  --as none
    #[arg(long = "as", value_name = "FORMAT", value_delimiter = ',', default_value = "pdf")]
    output_format: Vec<String>,

    /// Emit detailed progress logs while reading messages and writing attachments.
    #[arg(long)]
    verbose: bool,

    /// Filter attachment extensions. Use positive values to include only those
    /// extensions, or negative values to exclude them.
    #[arg(long, value_name = "EXT", value_delimiter = ',', allow_hyphen_values = true)]
    filter: Vec<String>,

    /// Recursively unpack compressed attachments into subfolders while keeping the originals.
    #[arg(long)]
    unzip: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum OutputBy {
    Flat,
    Conversation,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum OutputInto {
    One,
    Individual,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Derive the stem (base filename without extension) from the PST file.
    let pst_stem = args.pst
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
        .to_string();

    // Validate --as values
    let want_none = args.output_format.iter().any(|f| f.eq_ignore_ascii_case("none"));
    let want_pdf = args.output_format.iter().any(|f| f.eq_ignore_ascii_case("pdf"));
    let want_text = args.output_format.iter().any(|f| f.eq_ignore_ascii_case("text"));
    if want_none {
        if args.output_format.len() != 1 {
            eprintln!("error: --as none cannot be combined with pdf or text");
            std::process::exit(1);
        }
        if args.attachments.is_none() {
            eprintln!("error: --as none requires --attachments");
            std::process::exit(1);
        }
    } else if !want_pdf && !want_text {
        eprintln!("error: --as accepts 'pdf' and/or 'text' (e.g. --as pdf,text)");
        std::process::exit(1);
    }
    let attachment_filter = pst_reader::AttachmentFilter::from_specs(&args.filter);
    let by_conversation = matches!(args.by, OutputBy::Conversation);
    let into_individual = matches!(args.into, OutputInto::Individual);

    if want_none {
        if let Some(att_dir) = &args.attachments {
            if by_conversation {
                println!("Reading PST: {}", args.pst.display());
                let messages = pst_reader::read_messages(&args.pst, args.verbose)
                    .with_context(|| format!("Failed to read {}", args.pst.display()))?;

                println!("Found {} messages", messages.len());
                let threads = thread_grouper::group_by_thread(messages, args.verbose);
                println!("Grouped into {} conversation threads", threads.len());

                println!("Extracting attachments to: {}", att_dir.display());
                let count = pst_reader::save_attachments_for_threads(
                    &args.pst,
                    att_dir,
                    &threads,
                    &pst_stem,
                    &attachment_filter,
                    args.unzip,
                    args.verbose,
                )
                .with_context(|| format!("Failed to extract attachments to {}", att_dir.display()))?;
                println!("Saved {} attachment(s).", count);
            } else {
                println!("Reading PST: {}", args.pst.display());
                println!("Extracting attachments to: {}", att_dir.display());
                let count = pst_reader::save_attachments(
                    &args.pst,
                    att_dir,
                    &attachment_filter,
                    args.unzip,
                    args.verbose,
                )
                .with_context(|| format!("Failed to extract attachments to {}", att_dir.display()))?;
                println!("Saved {} attachment(s).", count);
            }
        }

        println!("Done.");
        return Ok(());
    }

    // Default output directory is the PST file's own directory.
    let pst_dir = args.pst
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    println!("Reading PST: {}", args.pst.display());
    let messages = pst_reader::read_messages(&args.pst, args.verbose)
        .with_context(|| format!("Failed to read {}", args.pst.display()))?;

    println!("Found {} messages", messages.len());
    if by_conversation {
        let threads = thread_grouper::group_by_thread(messages, args.verbose);
        println!("Grouped into {} conversation threads", threads.len());

        if into_individual {
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
        } else {
            let output_base = match args.output {
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
                let pdf_path = output_base.with_extension("pdf");
                println!("Writing PDF: {}", pdf_path.display());
                ensure_parent_dir(&pdf_path)?;
                pdf_writer::write_pdf(&threads, &pdf_path, args.showdetails)
                    .with_context(|| format!("Failed to write {}", pdf_path.display()))?;
            }

            if want_text {
                let txt_path = output_base.with_extension("txt");
                println!("Writing text: {}", txt_path.display());
                ensure_parent_dir(&txt_path)?;
                pdf_writer::write_text(&threads, &txt_path, args.showdetails)
                    .with_context(|| format!("Failed to write {}", txt_path.display()))?;
            }
        }

        if let Some(att_dir) = &args.attachments {
            println!("Extracting attachments to: {}", att_dir.display());
            let count = pst_reader::save_attachments_for_threads(
                &args.pst,
                att_dir,
                &threads,
                &pst_stem,
                &attachment_filter,
                args.unzip,
                args.verbose,
            )
                .with_context(|| format!("Failed to extract attachments to {}", att_dir.display()))?;
            println!("Saved {} attachment(s).", count);
        }
    } else {
        let messages = sort_flat_messages(messages);

        if into_individual {
            let output_dir = args.output.clone().unwrap_or_else(|| pst_dir.clone());

            if want_pdf {
                println!("Writing {} flat PDFs to: {}", messages.len(), output_dir.display());
                pdf_writer::write_flat_pdfs(&messages, &output_dir, &pst_stem, args.showdetails)
                    .with_context(|| format!("Failed to write flat PDFs to {}", output_dir.display()))?;
            }

            if want_text {
                println!("Writing {} flat text files to: {}", messages.len(), output_dir.display());
                pdf_writer::write_flat_texts(&messages, &output_dir, &pst_stem, args.showdetails)
                    .with_context(|| format!("Failed to write flat text files to {}", output_dir.display()))?;
            }
        } else {
            let output_base = match args.output {
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
                let pdf_path = output_base.with_extension("pdf");
                println!("Writing PDF: {}", pdf_path.display());
                ensure_parent_dir(&pdf_path)?;
                pdf_writer::write_flat_pdf(&messages, &pdf_path, args.showdetails)
                    .with_context(|| format!("Failed to write {}", pdf_path.display()))?;
            }

            if want_text {
                let txt_path = output_base.with_extension("txt");
                println!("Writing text: {}", txt_path.display());
                ensure_parent_dir(&txt_path)?;
                pdf_writer::write_flat_text(&messages, &txt_path, args.showdetails)
                    .with_context(|| format!("Failed to write {}", txt_path.display()))?;
            }
        }

        if let Some(att_dir) = &args.attachments {
            println!("Extracting attachments to: {}", att_dir.display());
            let count = pst_reader::save_attachments(
                &args.pst,
                att_dir,
                &attachment_filter,
                args.unzip,
                args.verbose,
            )
                .with_context(|| format!("Failed to extract attachments to {}", att_dir.display()))?;
            println!("Saved {} attachment(s).", count);
        }
    }

    println!("Done.");
    Ok(())
}
