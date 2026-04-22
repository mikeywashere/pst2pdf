# pst2pdf Test Suite

## Overview

This directory contains the comprehensive test suite for pst2pdf, a C++ command-line tool that converts Outlook PST files to PDF with email threads rendered as labeled sections.

**Test matrix:** See `test_matrix.md` for detailed test specifications (60+ test cases across 6 categories).

---

## Test Infrastructure Recommendation

### Recommended Approach: Layered Testing with Catch2 + Integration Framework

**The choice:** Separate unit tests (Catch2) for core logic + integration tests for end-to-end PST→PDF flow.

---

## Why This Architecture?

### What We're Testing

1. **Input validation** (CLI parsing) → fast, deterministic
2. **PST parsing** → complex, binary format, requires real or synthetic PST files
3. **Thread grouping logic** (normalization, sorting) → pure functions, fast
4. **PDF generation** → side effect (file I/O), requires visual/structural validation

### Why Catch2 + Integration

| Aspect | Choice | Reason |
|--------|--------|--------|
| **Unit tests** | Catch2 | Header-only, minimal setup, zero-copy captures, fast. Good for thread grouping logic. |
| **Thread grouping tests** | Catch2 (pure functions) | No I/O, no external deps, trivial to unit test |
| **PST parsing tests** | Integration + fixtures | Binary format is complex; real/synthetic PSTs required. Each test needs a small, known PST file. |
| **PDF validation** | Integration + tools | PDF output is opaque; validate via file structure (magic bytes), page count, content extraction |
| **CLI tests** | Integration (shell/script) | Easiest to test: spawn process, check exit code, verify output file exists |

---

## NOT Recommended

### GoogleTest
- **Why not:** Heavier than Catch2, no better for this use case, requires more configuration
- **When to reconsider:** If sharing test framework with large C++ codebase already using GoogleTest

### Unit-testing libHaru directly
- **Why not:** libHaru is external library; we own the integration, not the library
- **Better approach:** Test our PDF generation code + its libHaru calls via integration tests

### Mocking pstsdk
- **Why not:** PST parsing is core functionality; mocking defeats the purpose
- **Better approach:** Use small, controlled real PST files as fixtures

---

## Test Structure

```
tests/
├── README.md                          (this file)
├── test_matrix.md                     (specifications for all 60+ cases)
├── fixtures/                          (sample PST files for testing)
│   ├── single_message.pst
│   ├── multi_thread.pst
│   ├── reply_chain.pst
│   ├── forward_chain.pst
│   ├── unicode_content.pst
│   ├── no_subject.pst
│   ├── large_body.pst
│   ├── empty.pst
│   └── nested_folders.pst
├── unit/                              (Catch2 unit tests)
│   ├── CMakeLists.txt
│   ├── test_subject_normalization.cpp (thread grouping logic)
│   ├── test_message_sorting.cpp       (chronological ordering)
│   └── test_cli_parsing.cpp           (command-line argument validation)
├── integration/                       (Integration tests using fixtures)
│   ├── run_integration_tests.sh       (bash script to run tests)
│   ├── validate_pdf.py                (Python utility to inspect PDF structure)
│   └── test_cases.sh                  (shell script defining integration tests)
└── CMakeLists.txt                     (top-level test build config)
```

---

## Unit Tests (Catch2)

### What to test with Catch2

**Good candidates (pure functions, no I/O):**

1. **Subject normalization**
   ```cpp
   TEST_CASE("Normalize subject: RE: stripped", "[grouping]") {
       std::string normalized = normalize_subject("RE: RE: Meeting");
       REQUIRE(normalized == "meeting");
   }
   ```

2. **Thread grouping**
   ```cpp
   TEST_CASE("Group messages by normalized subject", "[grouping]") {
       std::vector<EmailMessage> messages = {...};
       auto threads = group_by_thread(messages);
       REQUIRE(threads.size() == 3);
   }
   ```

3. **Message sorting within thread**
   ```cpp
   TEST_CASE("Sort messages chronologically", "[sorting]") {
       std::vector<EmailMessage> unsorted = {msg3, msg1, msg2};
       auto sorted = sort_by_date(unsorted);
       REQUIRE(sorted[0].date < sorted[1].date);
   }
   ```

### Compile and run Catch2 tests

```bash
cd tests
mkdir -p build && cd build
cmake ..
cmake --build . --target unit_tests
./unit_tests
```

---

## Integration Tests

### Approach: Fixture-Driven + Shell Scripts

**Why shell/Python instead of C++?**
- Easier to verify file output (existence, size, format)
- Can invoke external tools (PDF validators)
- Don't need to link against application; test the final executable directly
- Cleaner separation of concerns

### Test execution

```bash
cd tests
./integration/run_integration_tests.sh
```

### Anatomy of an integration test

Each integration test follows this pattern:

1. **Setup:** Copy fixture PST to temp directory
2. **Execute:** Run `pst2pdf fixture.pst`
3. **Validate:** Check exit code, output PDF existence, content
4. **Cleanup:** Remove temp files

Example (shell script):

```bash
# Test: CMD-001 (No arguments)
echo "Test CMD-001: No arguments"
OUTPUT=$($PSTPDF_BIN 2>&1)
EXIT=$?
if [ $EXIT -eq 0 ]; then
    echo "FAIL: Expected non-zero exit code"
    exit 1
fi
if echo "$OUTPUT" | grep -q "Usage"; then
    echo "PASS"
else
    echo "FAIL: Usage message not found"
    exit 1
fi
```

### PDF Validation Utility

Python script `validate_pdf.py` to extract and verify PDF structure:

```bash
python3 tests/integration/validate_pdf.py output.pdf
# Output:
# Magic bytes: %PDF-1.4 ✓
# Valid structure: ✓
# Page count: 3
# Sections: ["Planning", "Budget", "Timeline"]
```

---

## Fixture Management

### Creating test fixtures

**Option 1: Programmatic (recommended)**
Use a PST library (e.g., `libpst` or equivalent) to create small, controlled fixtures:

```bash
# Example: Create a single-message PST
python3 create_fixtures.py --template single_message --output fixtures/single_message.pst
```

**Option 2: Real PSTs**
Use anonymized real-world PST files as fixtures. Ensure:
- No sensitive data (PII, credentials, etc.)
- Consistent, predictable structure for tests
- Stored in Git LFS if size > 100 KB

**Option 3: Hybrid**
Use programmatic generation for most cases, real PSTs for complex edge cases (nested folders, Unicode, etc.).

### Fixture inventory

| File | Purpose | Size | Notes |
|------|---------|------|-------|
| `single_message.pst` | PST-001 | < 1 KB | Minimal case; fast |
| `multi_thread.pst` | PST-002 | < 5 KB | 5 independent messages |
| `reply_chain.pst` | PST-003, GROUP-003, GROUP-004 | < 5 KB | RE: chain (4-5 messages) |
| `forward_chain.pst` | PST-004, PST-005 | < 3 KB | FW: + FWD: prefixes |
| `unicode_content.pst` | EDGE-006, EDGE-007, PDF-010 | < 5 KB | Japanese, Russian, accents |
| `no_subject.pst` | EDGE-002 | < 2 KB | Null and empty subjects |
| `large_body.pst` | EDGE-005, PDF-008 | < 100 KB | 10k+ word bodies |
| `empty.pst` | EDGE-001 | < 1 KB | Zero messages |
| `nested_folders.pst` | PST-006 | < 10 KB | Inbox, Sent, Custom folders |

---

## Executing Tests

### Phase 1: Unit tests (fast feedback)

```bash
cd tests/build
cmake ..
cmake --build .
./unit_tests  # ~1 second
```

### Phase 2: Integration tests (fixture-driven)

```bash
cd tests/integration
./run_integration_tests.sh
# Output:
# CMD-001: No arguments ..................... PASS
# CMD-002: Too many arguments .............. PASS
# CMD-003: Non-existent file ............... PASS
# ...
# Summary: 45/47 PASS (2 SKIP due to large file)
```

### Phase 3: Manual inspection (visual validation)

For PDF output tests, a human review is recommended:
- Open generated PDF in Adobe Reader or similar
- Verify section headings, message formatting, multi-page rendering

---

## Continuous Integration

### Recommended CI workflow

```yaml
# .github/workflows/test.yml
name: Test

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      # Build application
      - run: mkdir -p build && cd build && cmake .. && cmake --build .
      
      # Unit tests
      - run: cd tests/build && cmake .. && cmake --build . && ./unit_tests
      
      # Integration tests (select subset for CI speed)
      - run: cd tests/integration && ./run_integration_tests.sh --fast
      
      # Artifact: Sample PDF
      - uses: actions/upload-artifact@v3
        if: failure()
        with:
          name: test-output-pdf
          path: tests/output/*.pdf
```

---

## Known Limitations & Trade-offs

### Test matrix notes

1. **GROUP-006: False positive grouping**
   - Known limitation: Messages with identical normalized subjects are grouped even if unrelated
   - Acceptable trade-off: Simple heuristic vs. complex header-based threading
   - Users should understand this when using pst2pdf

2. **ROBUST-001: Large PST stress test**
   - Skipped in default CI (requires 50+ MB fixture)
   - Run locally on a dev machine with adequate resources
   - Included in release testing before shipping

3. **ROBUST-003, ROBUST-005, ROBUST-006: Advanced edge cases**
   - Difficult to test reliably in CI environment
   - Consider these as "nice to have" for initial release
   - Document expected behavior; manual testing if time permits

---

## Test Ownership & Maintenance

| Category | Owner | Run Frequency |
|----------|-------|---------------|
| Unit tests (Catch2) | Developer | Every commit (CI) |
| Integration tests | QA/Tester | Every PR |
| Fixture creation | QA/Tester | As needed |
| PDF visual validation | QA/Tester | Before release |
| Stress tests (ROBUST-001) | QA/Tester | Release testing |

---

## Debugging Failed Tests

### Unit test failure

```bash
cd tests/build
./unit_tests --help  # See Catch2 options
./unit_tests "[grouping]" --verbose  # Run just grouping tests with verbose output
```

### Integration test failure

```bash
cd tests/integration
bash -x run_integration_tests.sh 2>&1 | head -100  # Debug first failure
# Check temp directory: /tmp/pst2pdf_test_* (or tests/tmp/ on Windows)
ls -la /tmp/pst2pdf_test_*/
```

### PDF inspection

```bash
# Check magic bytes
od -c output.pdf | head -1

# Extract text
python3 -c "
import pdfplumber
with pdfplumber.open('output.pdf') as pdf:
    for i, page in enumerate(pdf.pages):
        print(f'Page {i}: {page.extract_text()[:100]}...')
"
```

---

## Performance Baselines

(To be filled in after first implementation)

| Test | Baseline | Warning | Critical |
|------|----------|---------|----------|
| Unit tests (all) | < 1 sec | > 2 sec | > 5 sec |
| Single message PST | < 100 ms | > 500 ms | > 2 sec |
| Multi-thread (10 threads) | < 500 ms | > 2 sec | > 5 sec |
| Large body (100 KB) | < 2 sec | > 5 sec | > 10 sec |
| Output PDF size (10 threads) | ~50 KB | > 100 KB | > 500 KB |

---

## Contributing New Test Cases

When adding a new test:

1. **Add test specification** to `test_matrix.md`
   - Assign test ID (CMD-NNN, PST-NNN, etc.)
   - Document input, expected behavior, pass criteria

2. **Create fixture** if needed
   - Add to `tests/fixtures/`
   - Document its purpose in fixture inventory above

3. **Implement test** in Catch2 or shell script
   - Unit test: `tests/unit/test_*.cpp`
   - Integration test: `tests/integration/test_cases.sh`

4. **Document edge case** in this README if special

---

## Summary: Test Strategy

**Guiding principle:** "Test the application as a user would, but verify internals where it matters."

- **CLI & file I/O:** Integration tests (simpler, clearer)
- **Core logic (grouping, sorting):** Unit tests (Catch2, fast)
- **Binary format (PST parsing):** Trust pstsdk; test with real fixtures
- **Visual output (PDF):** Structural validation (magic bytes, page count); manual review for layout

**Coverage goal:** 60+ test cases covering happy path, boundary conditions, and failure modes.

**Timeline:** Phase 1 (CLI + basic PST) ready before implementation. Phases 2-5 follow as implementation proceeds.

---

**Document Version:** 1.0  
**Created:** 2026-04-14  
**Owner:** Hank (Tester)  
**Next Review:** After Phase 1 implementation
