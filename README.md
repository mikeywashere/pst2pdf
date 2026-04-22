# pst2pdf

A C++17 command-line tool that reads an Outlook PST file and produces a PDF where each email conversation thread is rendered as its own labeled section.

## What it does

`pst2pdf` opens a `.pst` file, traverses all folders recursively (Inbox, Sent Items, Deleted Items, and any custom folders), extracts every email message, groups the messages into conversation threads by normalizing the subject line (stripping RE:/FW:/FWD: prefixes), and writes a PDF. Each thread occupies its own page. Within a thread, messages are listed chronologically with date, sender, recipients, subject, and body.

## Dependencies

| Dependency | Purpose |
|------------|---------|
| [pstsdk](https://github.com/enrondata/pstsdk) | C++ header-only library for reading PST/OST files |
| [libHaru (libharu)](http://libharu.org/) | C library for PDF generation |
| [Boost](https://www.boost.org/) ≥ 1.67 | Required by pstsdk (`filesystem`, `system`, `optional`) |
| CMake ≥ 3.16 | Build system |
| C++17 compiler | GCC ≥ 8, Clang ≥ 7, MSVC ≥ 2019 |

### Installing dependencies

**Ubuntu / Debian:**
```bash
sudo apt-get install libboost-filesystem-dev libboost-system-dev libharu-dev
# pstsdk is header-only; clone and note the include path:
git clone https://github.com/enrondata/pstsdk /opt/pstsdk
```

**macOS (Homebrew):**
```bash
brew install boost libharu
git clone https://github.com/enrondata/pstsdk /opt/pstsdk
```

**Windows (vcpkg):**
```powershell
vcpkg install boost-filesystem boost-system libharu
# pstsdk — clone manually:
git clone https://github.com/enrondata/pstsdk C:\opt\pstsdk
```

## Build

```bash
cmake -B build \
      -DPSTSDK_ROOT=/opt/pstsdk \
      -DLIBHARU_ROOT=/usr/local   # omit if found automatically
cmake --build build
```

On Windows with vcpkg:
```powershell
cmake -B build -DCMAKE_TOOLCHAIN_FILE=C:/vcpkg/scripts/buildsystems/vcpkg.cmake `
               -DPSTSDK_ROOT=C:/opt/pstsdk
cmake --build build --config Release
```

The binary is `build/pst2pdf` (or `build/Release/pst2pdf.exe` on Windows).

## Usage

```
pst2pdf <input.pst>
```

Produces `<input>.pdf` in the same directory as the input file.

**Example:**
```bash
./pst2pdf myarchive.pst
# → myarchive.pdf
```

Progress and warnings are printed to stdout/stderr:
```
Read 4231 messages from myarchive.pst
Found 812 conversation threads
PDF written to myarchive.pdf
```

## Known limitations

- **Thread grouping is a heuristic.** Messages are grouped by normalized subject (RE:/FW:/FWD: stripped, lowercased). Unrelated messages with the same subject will be merged, and related messages with edited subjects will be separated.
- **Attachments are not included.** Only the message body (plain text, or HTML with tags stripped) is rendered.
- **Wide characters are lossy.** Non-ASCII characters in sender names, subjects, and bodies are converted with a best-effort narrow conversion; characters outside ASCII are replaced with `?`. A proper UTF-8 → Latin-1 or full Unicode font embedding would be required for full internationalization.
- **pstsdk API compatibility.** pstsdk is a research-grade library with limited maintenance. The API calls in `pst_reader.cpp` are marked `// TODO: verify API` where the exact method names may differ between versions.
