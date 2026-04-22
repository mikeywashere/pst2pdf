# Project Context

- **Owner:** Michael R. Schmidt
- **Project:** pst2pdf — C++ command-line tool that reads an Outlook PST file and outputs a PDF where each email conversation thread is rendered as its own section.
- **Stack:** C++, pstsdk (PST parsing), libHaru (PDF generation), CMake
- **Created:** 2026-04-14

## Learnings

<!-- Append new learnings below. Each entry is something lasting about the project. -->

### 2026-04-14 — Initial pst2pdf implementation

**Intermediate model as decoupling boundary:**  
`EmailMessage` and `ConversationThread` are plain structs with no library types. `PstReader` resolves all pstsdk types before returning; `PdfWriter` owns all libHaru state. This is the load-bearing architectural decision — everything else can change without touching the model.

**pstsdk API is uncertain:**  
The original pstsdk (github.com/enrondata/pstsdk) is header-only and uses Boost. Key call sites are marked `// TODO: verify API`. The most likely-correct API: `pstsdk::database db(path)`, `db.open_root_folder()`, range-for over folder messages, `msg.get_subject()`, `msg.get_sender_name()`, `msg.get_sender_address()`, `msg.get_body()`, `msg.get_date()` returning `pstsdk::filetime`. Sub-folder iteration via `folder.sub_folder_list()`.

**FILETIME conversion:**  
pstsdk uses Windows FILETIME (100ns ticks since 1601-01-01). Subtract `116444736000000000ULL` then divide by `10000000` to get Unix time_t.

**libHaru layout strategy:**  
Track `y` position manually. Use `ensure_space(ctx, needed_pts)` before every draw call — if `y - needed < MARGIN`, call `HPDF_AddPage`. This gives transparent page flow without pre-computing page breaks. `longjmp` from the libHaru error callback is the standard pattern; set the `jmp_buf` before every libHaru operation block.

**Thread grouping:**  
`normalize_subject()` strips RE:/FW:/FWD: prefixes in a loop (not just once — subjects like "Re: Re: Re: foo" occur frequently). Then lowercase + trim. This is a static method so it can be tested in isolation.

**Deduplication:**  
PST files store messages in multiple folders. `std::unordered_set<std::string>` keyed on entry_id (bytes as string) is the right dedup structure — O(1) average per message.

**CMake for research libraries:**  
pstsdk has no CMake package config. Pattern: `find_path(PSTSDK_INCLUDE_DIR NAMES pstsdk/pst.h HINTS ${PSTSDK_ROOT}/include ...)` with a `FATAL_ERROR` and helpful message if not found. Expose `PSTSDK_ROOT` and `LIBHARU_ROOT` as cache variables for override.
