# pst2pdf Test Matrix

**Created:** 2026-04-14  
**Owner:** Hank (Tester)  
**Last Updated:** 2026-04-14  

This document defines the comprehensive test matrix for the pst2pdf application. Each test case specifies the conditions, expected behavior, and pass criteria. Tests are organized by category and designed to cover happy paths, edge cases, and failure modes.

---

## 1. COMMAND-LINE VALIDATION

Tests for CLI argument parsing and basic error handling before PST parsing.

### CMD-001: No arguments provided

**Description:** User runs `pst2pdf` with no arguments.

**Input Conditions:**
- Command: `pst2pdf`
- No positional arguments

**Expected Behavior:**
- Prints usage/help message to stdout or stderr
- Exits with non-zero status code (typically 1 or 2)
- Does not attempt to read any files

**Pass Criteria:**
- Exit code != 0
- Usage message printed
- Message contains hint of required argument (e.g., "Usage: pst2pdf <pst_file>")
- No data written to disk

**Notes:**
- Error message should be clear and actionable

---

### CMD-002: Too many arguments provided

**Description:** User runs `pst2pdf` with more than one argument.

**Input Conditions:**
- Command: `pst2pdf file1.pst file2.pst`
- Multiple positional arguments

**Expected Behavior:**
- Prints usage message
- Exits with non-zero status code
- Does not attempt to read any files

**Pass Criteria:**
- Exit code != 0
- Usage message printed
- No files processed

**Notes:**
- Error message should indicate "too many arguments" or similar

---

### CMD-003: Non-existent file

**Description:** User specifies a PST file path that doesn't exist.

**Input Conditions:**
- Command: `pst2pdf /path/to/nonexistent/file.pst`
- File path points to a file that does not exist on the filesystem

**Expected Behavior:**
- Prints error message indicating file not found
- Exits with non-zero status code
- Does not create a PDF output file

**Pass Criteria:**
- Exit code != 0
- Error message mentions "file not found" or "cannot open" or similar
- No output PDF created

**Notes:**
- Test with absolute paths and relative paths

---

### CMD-004: Input file is not a PST

**Description:** User specifies a file that exists but is not a valid PST file (e.g., ZIP, text file, PDF).

**Input Conditions:**
- Command: `pst2pdf some_file.zip` or `pst2pdf README.txt`
- File exists but is not a PST (wrong magic bytes or format)

**Expected Behavior:**
- Prints error message indicating invalid PST format
- Exits with non-zero status code
- Does not create a PDF output file

**Pass Criteria:**
- Exit code != 0
- Error message mentions "invalid PST" or "format error" or "not a PST file"
- No output PDF created

**Notes:**
- Test with a ZIP file as a common invalid input
- Test with a text file
- pstsdk should detect magic bytes and reject gracefully

---

### CMD-005: Output directory does not exist (read-only scenario)

**Description:** User specifies a PST file, but the output directory is read-only or inaccessible.

**Input Conditions:**
- Command: `pst2pdf /path/to/readonly/file.pst`
- Output path would be `/path/to/readonly/file.pdf`
- `/path/to/readonly/` directory exists but is read-only (permissions 0555 on Unix, or readonly attribute on Windows)

**Expected Behavior:**
- Prints error message indicating cannot write to output location
- Exits with non-zero status code
- Does not create a PDF output file (or creates it but cleans up on failure)

**Pass Criteria:**
- Exit code != 0
- Error message mentions "cannot write" or "permission denied" or similar
- No valid output PDF created in read-only directory

**Notes:**
- On Windows, test by setting file/folder attributes to read-only
- On Linux/Mac, use chmod to restrict permissions
- This tests error handling in libHaru/file I/O layer

---

## 2. PST PARSING — HAPPY PATH

Tests for correct parsing and thread grouping with standard, well-formed PST files.

### PST-001: Single message PST

**Description:** PST file contains exactly one email message.

**Input Conditions:**
- PST file with one message
- Message has: subject, sender, date, body
- No replies or related messages

**Expected Behavior:**
- Application successfully parses the PST
- Single ConversationThread created with one message
- PDF is generated with one section
- Exit code is 0

**Pass Criteria:**
- Exit code == 0
- Output PDF exists and is non-zero size
- PDF contains the message subject as a section heading
- PDF displays sender, date, and body
- Messages appear in correct order

**Notes:**
- This is the simplest happy path
- Requires sample PST with controlled content

---

### PST-002: Multiple independent threads

**Description:** PST contains multiple messages with distinct subjects (no replies).

**Input Conditions:**
- PST file with 5+ messages
- Each message has a unique subject
- No RE: or FW: prefixes
- Messages are not related

**Expected Behavior:**
- Application parses all messages successfully
- Each message becomes its own ConversationThread (normalization of subjects shows they're different)
- PDF has one section per thread
- Sections are clearly labeled with thread subject
- Exit code is 0

**Pass Criteria:**
- Exit code == 0
- Output PDF exists
- PDF contains N distinct sections (N = number of unique subjects)
- Each section has correct subject title
- All messages rendered

**Notes:**
- Tests that the application doesn't incorrectly merge unrelated messages

---

### PST-003: Reply chain (RE: subject)

**Description:** PST contains a sequence of replies to an original message.

**Input Conditions:**
- PST file with original message: subject = "Meeting tomorrow"
- Reply 1: subject = "RE: Meeting tomorrow"
- Reply 2: subject = "RE: RE: Meeting tomorrow"
- Messages are in chronological order (or out of order—test both)

**Expected Behavior:**
- Application groups all replies under a single ConversationThread
- Thread subject is "Meeting tomorrow" (normalized)
- All 3 messages appear in the thread
- PDF shows one section labeled "Meeting tomorrow"
- Messages appear in chronological order within the section

**Pass Criteria:**
- Exit code == 0
- PDF contains 1 section titled "Meeting tomorrow"
- Section contains all 3 messages
- Messages in thread are in chronological order (earliest first)
- RE: prefixes are stripped from display subject

**Notes:**
- Critical test for core functionality
- Normalization must handle "RE: RE: RE:" correctly
- Order may vary in PST; application should sort by date

---

### PST-004: Forward chain (FW: subject)

**Description:** PST contains an original message and a forward.

**Input Conditions:**
- PST file with original: subject = "Sales update"
- Forward: subject = "FW: Sales update"
- Both messages present

**Expected Behavior:**
- Application groups forward and original under a single thread
- Thread subject normalized to "Sales update"
- Both messages in the thread
- PDF shows one section

**Pass Criteria:**
- Exit code == 0
- PDF contains 1 section
- Section subject is "Sales update" (FW: stripped)
- Both messages rendered

**Notes:**
- FW: prefix is a common variation of FWD:
- Tests prefix stripping logic

---

### PST-005: FWD: prefix (alternative forward)

**Description:** PST contains a message with FWD: prefix (instead of FW:).

**Input Conditions:**
- PST file with original: subject = "Budget request"
- Message with subject = "FWD: Budget request"

**Expected Behavior:**
- Grouped as one thread
- Subject normalized to "Budget request"

**Pass Criteria:**
- Exit code == 0
- PDF contains 1 section titled "Budget request"
- Both messages in section

**Notes:**
- Tests that both FW: and FWD: are recognized

---

### PST-006: Nested folder structure

**Description:** PST contains messages in subfolders (Inbox, Sent Items, Custom folders, etc.).

**Input Conditions:**
- PST with:
  - Message in /Inbox: subject = "Project discussion"
  - Message in /Sent Items: subject = "RE: Project discussion"
  - Message in /Custom Folder: subject = "RE: RE: Project discussion"

**Expected Behavior:**
- Application recursively reads all folders
- All messages included regardless of folder location
- Messages grouped into single thread by subject
- PDF contains all messages

**Pass Criteria:**
- Exit code == 0
- All 3 messages appear in output
- All appear in one section
- Chronological order maintained

**Notes:**
- PST file structure includes folder hierarchy
- Application must not skip subfolders
- Real PST files have complex folder structures

---

### PST-007: Messages in order vs. out of order

**Description:** PST contains reply chain messages not in chronological order in the file.

**Input Conditions:**
- PST file with messages in this order:
  1. Message 3 (date: 2026-04-14 15:00, subject: "RE: RE: Status")
  2. Message 1 (date: 2026-04-14 10:00, subject: "Status")
  3. Message 2 (date: 2026-04-14 12:00, subject: "RE: Status")

**Expected Behavior:**
- Thread groups all three messages
- PDF renders them in chronological order (1→2→3 by date, not file order)

**Pass Criteria:**
- Exit code == 0
- PDF shows messages in order: Message 1 → Message 2 → Message 3
- All messages in one section

**Notes:**
- Real PST files don't guarantee chronological order
- Date field must be parsed and used for sorting within a thread

---

## 3. PST PARSING — EDGE CASES

Tests for boundary conditions, missing data, and malformed input.

### EDGE-001: Empty PST

**Description:** PST file exists but contains no messages.

**Input Conditions:**
- Valid PST file format
- Zero messages in all folders

**Expected Behavior:**
- Application recognizes empty PST
- Does not crash
- Generates PDF with a message like "No conversations found" or empty PDF
- Exits with code 0 (graceful handling, not an error)

**Pass Criteria:**
- Exit code == 0
- Output PDF created
- PDF either shows "No conversations found" message or is empty (application design choice)
- No crash or exception

**Notes:**
- Graceful handling of degenerate case
- Empty PST is technically valid

---

### EDGE-002: Message with no subject

**Description:** PST contains messages without a subject line.

**Input Conditions:**
- Message 1: subject = "" (empty string)
- Message 2: subject = (null/missing)
- Both in same PST

**Expected Behavior:**
- Application handles missing subject gracefully
- Messages grouped under a default thread like "(No Subject)" or empty string
- All no-subject messages group together
- PDF renders them correctly

**Pass Criteria:**
- Exit code == 0
- Messages appear in PDF
- All no-subject messages in same section
- Section title is visible and indicates no-subject grouping

**Notes:**
- Real-world edge case: some spam or auto-generated messages have no subject
- Normalization logic must handle null/empty

---

### EDGE-003: Message with empty body

**Description:** PST contains a message with subject but no body text.

**Input Conditions:**
- Message with subject = "Brief hello"
- Message body = "" (empty)

**Expected Behavior:**
- Message parsed successfully
- PDF renders with subject and empty body section
- No crash

**Pass Criteria:**
- Exit code == 0
- Message appears in PDF
- Subject rendered
- Body section is empty but present (not a rendering error)

**Notes:**
- Common in auto-generated emails

---

### EDGE-004: Very long subject (300+ characters)

**Description:** PST contains message with extremely long subject.

**Input Conditions:**
- Message with subject = 350-character string (exceeds typical column width)

**Expected Behavior:**
- Subject parsed in full
- PDF renders subject (possibly with wrapping)
- No truncation of subject in data
- PDF is valid and viewable

**Pass Criteria:**
- Exit code == 0
- Full subject present in PDF (not truncated)
- PDF renders without corruption
- Text wraps or fits on page

**Notes:**
- Tests libHaru text rendering limits
- Subject should not be silently truncated (log warning if desired)

---

### EDGE-005: Very long body (10,000+ words)

**Description:** PST contains message with very large body (100+ KB of text).

**Input Conditions:**
- Message with body = 10,000+ word email (e.g., pasted article, log file)

**Expected Behavior:**
- Body parsed in full
- PDF generated (possibly multi-page)
- No data loss or truncation
- PDF is valid and viewable

**Pass Criteria:**
- Exit code == 0
- Output PDF contains full message body
- PDF is multi-page if necessary
- PDF renders without corruption
- File size reasonable (not exponentially expanded)

**Notes:**
- Tests libHaru multi-page handling
- Real-world edge case: forwarded emails with large attachments-as-text or quoted messages

---

### EDGE-006: Unicode characters in subject

**Description:** PST contains messages with non-ASCII characters (accented, CJK, emoji, etc.).

**Input Conditions:**
- Message 1: subject = "Café déjeuner réunion" (Latin-1 accents)
- Message 2: subject = "プロジェクト進捗状況" (Japanese)
- Message 3: subject = "Привет коллеги" (Russian Cyrillic)
- Message 4: subject = "Test 🎉 emoji" (emoji)

**Expected Behavior:**
- All messages parsed correctly
- Unicode characters preserved in PDF
- No mojibake or encoding errors
- Messages still group correctly by normalized subject

**Pass Criteria:**
- Exit code == 0
- PDF renders all Unicode subjects correctly
- No corrupted characters
- Messages appear in correct sections
- Test on Windows (UTF-16 PST) and with various locales

**Notes:**
- Outlook PST format supports both ANSI and Unicode
- libHaru may require UTF-8 or specific encoding
- Critical for international users

---

### EDGE-007: Unicode in message body

**Description:** PST contains message bodies with non-ASCII text.

**Input Conditions:**
- Message body with mixed languages: "Hola amigos, 你好朋友, привет"

**Expected Behavior:**
- Body parsed and rendered correctly
- No encoding errors in PDF

**Pass Criteria:**
- Exit code == 0
- Body text renders correctly in PDF
- No corrupted characters

**Notes:**
- Tests full pipeline: PST parsing → data model → PDF rendering

---

### EDGE-008: Null/special characters in sender or to fields

**Description:** PST contains messages with unusual sender or recipient data.

**Input Conditions:**
- Message with sender_name = null, sender_address = "unknown@example.com"
- Message with to_addresses = [] (empty list)
- Message with malformed email: to_addresses = ["not-an-email"]

**Expected Behavior:**
- Messages parse without crashing
- PDF renders with available data (sender address shown even if name missing)
- Missing/invalid addresses handled gracefully

**Pass Criteria:**
- Exit code == 0
- All messages appear in PDF
- Sender info displayed as available
- To list may be empty but doesn't break rendering

**Notes:**
- Real-world case: system-generated messages often have odd addressing

---

### EDGE-009: Message with no date

**Description:** PST contains message with missing or invalid date field.

**Input Conditions:**
- Message with date_str = "" or null

**Expected Behavior:**
- Message parsed
- Sorting within thread handles missing date gracefully (use insertion order or skip)
- PDF renders message with "Date: unknown" or similar

**Pass Criteria:**
- Exit code == 0
- Message appears in PDF
- Doesn't break thread ordering

**Notes:**
- Rare but possible in corrupted PST files or certain edge cases

---

## 4. THREAD GROUPING

Tests for the normalization and grouping logic specifically.

### GROUP-001: Identical subjects without RE:/FW: prefix

**Description:** Multiple messages with identical exact subjects (no prefixes).

**Input Conditions:**
- Message 1: subject = "Meeting notes"
- Message 2: subject = "Meeting notes"
- Message 3: subject = "Meeting notes"

**Expected Behavior:**
- All three grouped into single thread
- Thread subject = "Meeting notes"

**Pass Criteria:**
- Single section in PDF titled "Meeting notes"
- Contains 3 messages
- Correct count of sections in PDF

**Notes:**
- Basic test that normalization works for exact matches

---

### GROUP-002: Case-insensitive grouping

**Description:** Messages with the same subject but different case.

**Input Conditions:**
- Message 1: subject = "Project Alpha"
- Message 2: subject = "project alpha"
- Message 3: subject = "PROJECT ALPHA"

**Expected Behavior:**
- All three grouped into single thread
- Thread subject displayed as-is from first message (or normalized)

**Pass Criteria:**
- Single section in PDF
- All 3 messages in that section
- Section title present and legible

**Notes:**
- Normalization must be case-insensitive
- Important because users and mail clients use different casing

---

### GROUP-003: Multiple RE: prefixes

**Description:** Reply chains with multiple levels of RE:.

**Input Conditions:**
- Message 1: subject = "Deadline"
- Message 2: subject = "RE: Deadline"
- Message 3: subject = "RE: RE: Deadline"
- Message 4: subject = "RE: RE: RE: Deadline"

**Expected Behavior:**
- All grouped into single thread
- Thread subject normalized to "Deadline"

**Pass Criteria:**
- Single section titled "Deadline"
- 4 messages in the section
- All RE: prefixes removed from display

**Notes:**
- Tests that normalization strips all leading RE: prefixes, not just one

---

### GROUP-004: Mixed RE: and FW: in same thread

**Description:** A message forwarded and then replied to (or vice versa).

**Input Conditions:**
- Message 1: subject = "Status update"
- Message 2: subject = "FW: Status update"
- Message 3: subject = "RE: FW: Status update"

**Expected Behavior:**
- All three in single thread
- Thread subject normalized to "Status update"

**Pass Criteria:**
- Single section
- 3 messages
- Subject correctly normalized

**Notes:**
- Real-world scenario: someone forwards a message, others reply to the forward

---

### GROUP-005: Whitespace handling in subject normalization

**Description:** Subjects with leading/trailing/internal whitespace.

**Input Conditions:**
- Message 1: subject = "  Whitespace test  " (leading/trailing spaces)
- Message 2: subject = "Whitespace test" (no extra spaces)
- Message 3: subject = "Whitespace  test" (multiple internal spaces, rare but possible)

**Expected Behavior:**
- All grouped into single thread
- Whitespace normalized (leading/trailing trimmed, internal spaces collapsed or preserved consistently)

**Pass Criteria:**
- Single section
- 3 messages
- Display subject is clean (not showing leading/trailing spaces)

**Notes:**
- Edge case: mail systems sometimes add/strip spaces

---

### GROUP-006: Known limitation: false positive grouping

**Description:** Document the known limitation that messages with the same subject are grouped even if they're unrelated conversations.

**Input Conditions:**
- Two unrelated email threads happen to have the same subject
- Message 1a: subject = "Status", date = 2026-01-01, from person A
- Message 1b: subject = "Status", date = 2026-02-01, from person B
- Both are "Status" but completely independent (different thread context)

**Expected Behavior:**
- Application groups both under "Status" (current limitation)
- Not a failure, but a known design trade-off

**Pass Criteria:**
- Both appear in same section
- This is acceptable per requirements (thread grouping is heuristic-based)
- Document this in README

**Notes:**
- True threading would require Message-ID and In-Reply-To headers
- Current approach assumes "same normalized subject = same thread"
- This is a reasonable heuristic but can have false positives
- Document clearly so users understand limitation

---

## 5. PDF OUTPUT

Tests for PDF generation, structure, and correctness.

### PDF-001: Output file creation at expected path

**Description:** Check that the output PDF is created with the correct filename/path.

**Input Conditions:**
- Input: `/home/user/mail.pst`

**Expected Behavior:**
- Output: `/home/user/mail.pdf`
- File created in same directory as input
- Correct name (input basename + .pdf extension)

**Pass Criteria:**
- Output file exists at expected path
- File extension is .pdf
- File path matches expected location

**Notes:**
- Test with both relative and absolute paths
- Windows paths with backslashes, Unix with forward slashes

---

### PDF-002: Output file is not zero bytes

**Description:** Ensure the generated PDF has content.

**Input Conditions:**
- PST with at least one message

**Expected Behavior:**
- Output PDF file size > 0 bytes
- (Ideally size is reasonable, not excessively large)

**Pass Criteria:**
- File size > 0
- File size < 100MB (reasonable upper bound for safety)

**Notes:**
- Catches failures in PDF writing where file is created but empty

---

### PDF-003: PDF is valid and readable

**Description:** Check that the output PDF is a valid PDF file that can be opened.

**Input Conditions:**
- Any valid test PST

**Expected Behavior:**
- PDF is valid PDF format (starts with %PDF magic bytes)
- Can be opened by a PDF reader (validation tool)
- No corruption

**Pass Criteria:**
- File magic bytes are %PDF-1.x
- PDF structure is valid (can be parsed by a PDF validator)
- No syntax errors when loaded with a PDF library

**Notes:**
- Use a PDF validator tool or library to check validity
- libHaru should produce valid PDFs, but worth verifying end-to-end

---

### PDF-004: Each thread appears as a section

**Description:** Verify each conversation thread has a corresponding section in the PDF.

**Input Conditions:**
- PST with 3 independent threads (different subjects)

**Expected Behavior:**
- PDF contains 3 sections
- Each section corresponds to one thread
- Sections are clearly delineated (section headers, spacing, etc.)

**Pass Criteria:**
- Exactly 3 sections in PDF (count section headings)
- Each section has a title matching the thread subject
- Sections are visually distinct

**Notes:**
- Requires PDF analysis tool or manual inspection
- Section structure should be unambiguous

---

### PDF-005: Thread subject displayed correctly

**Description:** Thread subject appears as a section heading in PDF.

**Input Conditions:**
- PST with message: subject = "Q2 Planning"

**Expected Behavior:**
- PDF section heading shows "Q2 Planning"
- Formatting makes it clear this is a section heading (font size, bold, etc.)

**Pass Criteria:**
- Subject text appears in PDF
- Formatted as a heading (visually distinct from body text)
- Complete (not truncated)

**Notes:**
- libHaru should render this; test that font/size/bold are set correctly

---

### PDF-006: Messages within thread in chronological order

**Description:** Messages in a thread are displayed in chronological order in the PDF (earliest first).

**Input Conditions:**
- PST with reply chain:
  - Message 1: date = 2026-04-14 09:00
  - Message 2: date = 2026-04-14 11:00
  - Message 3: date = 2026-04-14 13:00

**Expected Behavior:**
- PDF displays messages in this order: Message 1 → Message 2 → Message 3

**Pass Criteria:**
- Messages appear in date order in PDF
- Can verify by checking timestamps in PDF or manually inspecting

**Notes:**
- Critical for readability of conversation

---

### PDF-007: Message metadata displayed (sender, date, body)

**Description:** Each message shows sender, date, and body in the PDF.

**Input Conditions:**
- Message with:
  - sender_name = "Alice Smith"
  - sender_address = "alice@example.com"
  - date_str = "2026-04-14 10:30 AM"
  - body = "Can we schedule a meeting?"

**Expected Behavior:**
- PDF displays all four fields for the message
- Format is clear and readable
- Visually distinguishes from next message

**Pass Criteria:**
- "Alice Smith" (or "alice@example.com") visible in PDF
- Date "2026-04-14 10:30 AM" visible
- Body text "Can we schedule a meeting?" visible
- Message is clearly delineated from others

**Notes:**
- Specific format (layout, spacing) is application design choice
- Test that all key info is present

---

### PDF-008: Multi-page thread (page overflow)

**Description:** When a thread has many messages or messages with large bodies, verify PDF correctly spans multiple pages.

**Input Conditions:**
- PST with single thread containing 20+ messages with substantial bodies (total content > 1 page worth)

**Expected Behavior:**
- PDF creates multiple pages
- Thread continues across pages
- Page breaks handled gracefully (no data loss)
- Pages are connected (same thread, sequential)

**Pass Criteria:**
- Output PDF has multiple pages
- All messages rendered
- No data loss
- Page numbering or threading makes it clear pages are part of same thread

**Notes:**
- Tests libHaru's multi-page handling
- Real-world case: long email threads

---

### PDF-009: Multiple threads, section ordering

**Description:** When PDF has multiple threads, verify they appear in a logical order.

**Input Conditions:**
- PST with threads:
  - Thread A: "Planning" (5 messages)
  - Thread B: "Budget" (3 messages)
  - Thread C: "Timeline" (2 messages)

**Expected Behavior:**
- PDF sections appear in a consistent order
- Order is either: earliest date first, or alphabetical, or insertion order (application design choice)

**Pass Criteria:**
- Threads appear in consistent order
- Order is documented
- Easy to locate threads in PDF

**Notes:**
- Design choice: document in README which order is used

---

### PDF-010: Special characters in PDF rendering

**Description:** Test that special characters (é, ñ, 中文, etc.) render correctly in PDF.

**Input Conditions:**
- Thread subject = "Café rendezvous"
- Message sender = "José García"
- Message body = "See attached: 日本語 text"

**Expected Behavior:**
- All special characters display correctly in PDF
- No mojibake or corruption

**Pass Criteria:**
- PDF renders special characters correctly
- Can read and identify characters
- No encoding errors visible

**Notes:**
- Depends on fonts available in libHaru
- May require setting appropriate font in PDF generation

---

## 6. ROBUSTNESS

Tests for error handling, resource constraints, and unusual conditions.

### ROBUST-001: Large PST file (50+ MB)

**Description:** PST file is large (millions of messages or total size > 50 MB).

**Input Conditions:**
- Valid PST file, size = 50+ MB
- Contains 1000+ messages

**Expected Behavior:**
- Application parses successfully
- Memory usage is reasonable (no unbounded allocation)
- PDF generated
- No timeout or crash

**Pass Criteria:**
- Exit code == 0
- PDF generated
- Reasonable runtime (< 60 seconds)
- Reasonable memory (< 1 GB if possible)

**Notes:**
- Performance test; real PSTs can be very large
- May need to run on a machine with sufficient resources

---

### ROBUST-002: Permissions error on input file

**Description:** PST file exists but is not readable (permission denied).

**Input Conditions:**
- PST file with read permissions removed (chmod 000 on Unix, or readonly on Windows)

**Expected Behavior:**
- Application detects permission error
- Prints error message
- Exits with non-zero code

**Pass Criteria:**
- Exit code != 0
- Error message indicates permission denied or cannot read file
- No output PDF created

**Notes:**
- Test on target OS
- Requires ability to change file permissions in test environment

---

### ROBUST-003: PST file modified while reading

**Description:** (Advanced/rare) PST file is modified or deleted during reading.

**Input Conditions:**
- Start reading PST
- While reading, file is deleted or truncated

**Expected Behavior:**
- Application detects error (file no longer available)
- Error message printed
- Exits with non-zero code
- Partial output (if any) cleaned up

**Pass Criteria:**
- Exit code != 0
- Error message printed
- No incomplete/corrupted output PDF left behind

**Notes:**
- Difficult to test reliably; may skip in initial test suite
- More of an edge case for integration tests

---

### ROBUST-004: Corrupted PST file (partial/truncated)

**Description:** PST file is malformed or truncated (not a complete valid PST).

**Input Conditions:**
- File header indicates PST, but file is truncated or has corrupted bytes

**Expected Behavior:**
- pstsdk detects corruption
- Application prints error message
- Exits with non-zero code

**Pass Criteria:**
- Exit code != 0
- Error message indicates corruption or parse error
- No output PDF created

**Notes:**
- Requires test case: valid PST file that's been truncated
- Tests pstsdk robustness

---

### ROBUST-005: Signal handling (graceful shutdown)

**Description:** Application receives a signal (SIGINT, SIGTERM) during processing.

**Input Conditions:**
- Start pst2pdf on large PST
- Send SIGINT (Ctrl+C) during processing

**Expected Behavior:**
- Application catches signal
- Cleans up resources
- Exits cleanly
- Partial output cleaned up if desired

**Pass Criteria:**
- Exit code indicates interruption (typically 130 for SIGINT)
- No crashed processes or zombie threads
- Partial output cleaned up

**Notes:**
- Requires ability to send signals in test environment
- May be optional for initial test suite
- Tests signal handler implementation

---

### ROBUST-006: Disk full during PDF writing

**Description:** Output disk is full while writing PDF.

**Input Conditions:**
- Output directory on a filesystem that becomes full during write

**Expected Behavior:**
- libHaru detects write error
- Application prints error message
- Exits with non-zero code
- Partial file cleaned up

**Pass Criteria:**
- Exit code != 0
- Error message indicates disk full or write error
- No incomplete file left behind (or file is cleaned up)

**Notes:**
- Difficult to test in typical environment (requires controlling disk space)
- May be skipped in initial suite; useful for stress testing

---

## Test Infrastructure Recommendations

(See README.md for detailed recommendations)

**Summary:**
- **Unit tests:** Catch2 (C++, header-only, simple)
- **Integration tests:** Manual PST files + automated PDF validation
- **Approach:** Separate layers for PST parsing, thread grouping, and PDF rendering
- **Test PST files:** Small, reproducible sample PSTs in `tests/fixtures/`

---

## Test Execution Plan

### Phase 1: Command-line & basic PST parsing (priority: HIGH)
- CMD-001, CMD-002, CMD-003, CMD-004
- PST-001, PST-002, PST-003

### Phase 2: Thread grouping & PDF output (priority: HIGH)
- GROUP-001 through GROUP-005
- PDF-001 through PDF-007

### Phase 3: Edge cases (priority: MEDIUM)
- EDGE-001 through EDGE-009
- All Unicode/encoding tests

### Phase 4: Robustness & stress (priority: LOW)
- ROBUST-001 through ROBUST-006

### Phase 5: Polish & regression (priority: LOW)
- Fine-tune based on implementation details
- Add regression tests for any bugs found

---

## Fixture Files

The following PST test fixtures should be created or obtained:

| Fixture | Description | Size | Messages | Notes |
|---------|-------------|------|----------|-------|
| `single_message.pst` | One message | < 1 KB | 1 | Simplest case |
| `multi_thread.pst` | 5 independent threads | < 5 KB | 5 | Different subjects |
| `reply_chain.pst` | RE: chain with 5 messages | < 5 KB | 5 | Tests grouping |
| `forward_chain.pst` | FW: chain | < 3 KB | 2 | Tests FW: grouping |
| `unicode_content.pst` | Japanese, Russian, accents | < 2 KB | 3 | Encoding tests |
| `no_subject.pst` | Messages without subject | < 2 KB | 3 | Edge case |
| `large_body.pst` | Messages with 5000+ word bodies | < 100 KB | 2 | Page overflow test |
| `empty.pst` | Zero messages | < 1 KB | 0 | Empty PST |
| `nested_folders.pst` | Messages in subfolders | < 10 KB | 10 | Folder recursion |

Fixtures can be created programmatically using a PST library or obtained as real-world samples (anonymized).

---

**Document Version:** 1.0  
**Date:** 2026-04-14  
**Owner:** Hank (Tester)  
**Status:** Ready for review
