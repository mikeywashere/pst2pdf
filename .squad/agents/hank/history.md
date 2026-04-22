# Project Context

- **Owner:** Michael R. Schmidt
- **Project:** pst2pdf — C++ command-line tool that reads an Outlook PST file and outputs a PDF where each email conversation thread is rendered as its own section.
- **Stack:** C++, pstsdk (PST parsing), libHaru (PDF generation), CMake
- **Created:** 2026-04-14

## Learnings

### Test Infrastructure Decision (2026-04-14)
- **Chose:** Layered testing with Catch2 (unit tests) + shell/Python (integration tests)
- **Why:** PST parsing is binary format → real fixtures essential (not mockable); thread grouping is pure logic → unit-testable with Catch2; CLI/PDF I/O → integration tests simpler and clearer
- **Trade-off:** Fixture management overhead pays off with reproducible, fast-running tests; PDF validation is structural (magic bytes, page count), not visual
- **Key insight:** 60 test cases naturally split: 7 unit (logic), 53 integration (I/O + real files). Monolithic approach would sacrifice either speed or realism.
- **Recommendation to future testers:** Start with fixtures early (even simple ones). PST parsing is the unknowable; reduce surprises with real/synthetic samples.

### Test Matrix Design (2026-04-14)
- **Coverage:** 60+ test cases across 6 categories: CLI validation (5), PST parsing happy path (7), edge cases (9), thread grouping (6), PDF output (10), robustness (6)
- **Critical tests:** CMD-001 through CMD-004 (fail fast on bad args); PST-003 (reply chain grouping); GROUP-001 through GROUP-005 (core logic); PDF-001 through PDF-007 (output correctness)
- **Known limitation documented:** GROUP-006 (false positive grouping when unrelated threads share subject). Acceptable heuristic-based trade-off; users warned.
- **Fixture list:** 9 small PSTs (< 100 KB total) cover all test paths. Programmatic creation recommended over real-world sampling.

### Edge Cases Worth Testing (2026-04-14)
- **Unicode:** PST supports both ANSI and Unicode; test both (Latin-1 accents, CJK, Cyrillic). Font handling in libHaru is the unknown.
- **Empty/null:** Messages with no subject group under "(No Subject)"; no-body messages render empty (not error); empty PST gracefully shows message.
- **Large content:** 10k+ word bodies must be multi-page in PDF (libHaru's responsibility); verify no truncation or corruption.
- **Threading limitation:** Current design groups by normalized subject only (no Message-ID/In-Reply-To). False positives acceptable; document clearly.

### Test Phases & Prioritization (2026-04-14)
- **Phase 1 (HIGH):** CLI + basic PST (CMD-001-004, PST-001-003). Implement before full app to catch basic failures early.
- **Phase 2 (HIGH):** Thread grouping + PDF output (GROUP-001-005, PDF-001-007). Core functionality; tests drive implementation.
- **Phase 3 (MEDIUM):** Edge cases (EDGE-001-009). Robustness increases; catch unusual but real scenarios.
- **Phase 4 (LOW):** Stress + robustness (ROBUST-001-006). Optional in MVP; essential before shipping to end users.

### Fixture Management (2026-04-14)
- **Recommendation:** Programmatic PST creation (e.g., Python libpst wrapper) > real-world samples (privacy concerns, large) > hand-crafted binary (brittle, hard to maintain).
- **Storage:** Fixtures < 100 KB total; check into Git. Larger fixtures (50+ MB for stress test) can be generated on-demand or run locally.
- **Versioning:** Each fixture has clear "purpose" (e.g., `reply_chain.pst` is RE: chain with 5 messages). Don't mutate; create new fixture for new scenario.

### PDF Validation (2026-04-14)
- **Structural validation:** Magic bytes (%PDF-1.x), page count, section headings, message count. Automatable, fast.
- **Content validation:** Extract text with pdfplumber or similar; verify subjects, dates, senders appear in expected order.
- **Visual validation:** Manual human review for layout, fonts, wrapping. Recommended before release but not in automated CI.
- **Avoid:** Don't try to pixel-match PDFs (brittle, font-dependent). Verify content and structure instead.

### CI/CD Considerations (2026-04-14)
- **Fast path:** Unit tests (< 1 sec) + small integration tests (< 10 sec). Suitable for every commit.
- **Full path:** Add large fixture stress test (> 60 sec) for pre-release testing only.
- **Artifacts:** Save sample PDFs on test failure for inspection. GitHub Actions `upload-artifact` recommended.
- **Flaky tests:** Shell-based tests can fail on whitespace/ordering. Use robust parsing (grep/Python) not string matching.
