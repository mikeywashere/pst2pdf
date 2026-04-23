# pst2pdf

A Rust command-line tool that converts an Outlook PST email archive into PDF. It traverses every folder in the PST, groups messages into conversation threads, and writes the result as a readable, formatted PDF.

## Features

- Recursively traverses all PST folders (Inbox, Sent Items, custom folders, etc.)
- Groups messages into conversation threads by normalized subject
- Outputs a single combined PDF, plain text, **or** both
- Exports attachments to a folder, with optional per-conversation numbering
- Verbose mode for step-by-step processing logs
- Attachment extension filters with include and exclude rules
- Filters internal Exchange DN addresses (`/O=…/CN=…`) from display by default
- Supports both Unicode and ANSI PST formats

## Build

Requires [Rust](https://rustup.rs/) (stable).

```powershell
cargo build --release
```

The binary will be at `target\release\pst2pdf.exe`.

## Usage

```
pst2pdf --pst <FILE> [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--pst <FILE>` | Path to the PST file to convert *(required)* |
| `--output <FILE>` | Output PDF path (default: same name as PST with `.pdf` extension) |
| `--conversations` | Write one PDF per conversation thread instead of one combined PDF |
| `--attachments <DIR>` | Extract attachments into this folder |
| `--showdetails` | Show raw Exchange DN addresses in From/To fields |
| `--as <text,pdf>` | Choose output format(s): `pdf`, `text`, or both (comma-separated). Default: `pdf` |
| `--verbose` | Print detailed progress logs while reading messages and attachments |
| `--filter <EXTS>` | Filter attachment extensions. Use `png,eml` to include only those, or `-emz,-bmp` to exclude those |

Running with no arguments prints help.

## Examples

**Convert a PST to a single PDF:**
```shell
pst2pdf --pst myarchive.pst
# → myarchive.pdf
```

**Specify an output path:**
```shell
pst2pdf --pst myarchive.pst --output C:\exports\email.pdf
```

**Write text instead of PDF:**
```shell
pst2pdf --pst myarchive.pst --as text
# → myarchive.txt
```

**Write both PDF and text:**
```shell
pst2pdf --pst myarchive.pst --as pdf,text
# → myarchive.pdf
# → myarchive.txt
```

**One PDF per conversation thread:**
```shell
pst2pdf --pst myarchive.pst --conversations --output C:\exports\myarchive.pdf
# → C:\exports\myarchive-00001.pdf
# → C:\exports\myarchive-00002.pdf
# → ...
```

**Export attachments alongside conversation PDFs:**
```shell
pst2pdf --pst myarchive.pst --conversations --attachments C:\exports\attachments
# Attachments are prefixed with the conversation number:
# → C:\exports\attachments\myarchive-00003-photo.jpg
```

**Show internal Exchange addresses:**
```shell
pst2pdf --pst myarchive.pst --showdetails
```

**Verbose logging:**
```shell
pst2pdf --pst myarchive.pst --verbose
```

**Filter attachments by extension:**
```shell
pst2pdf --pst myarchive.pst --attachments C:\exports\attachments --filter png,eml,txt,msg
pst2pdf --pst myarchive.pst --attachments C:\exports\attachments --filter -emz,-bmp
```

## Notes

- Thread grouping is heuristic: messages are grouped by normalized subject (RE:/FW:/FWD: prefixes stripped, case-folded). Unrelated messages sharing a subject will be merged; related messages with edited subjects will be split.
- Non-Latin-1 characters in names, subjects, and bodies are replaced with `?` (built-in PDF fonts are Windows-1252).
- Attachment extraction uses the MS-PST spec §2.4.6.1.3 approach: each attachment's sub-node NID is read directly from the attachment table row ID.
