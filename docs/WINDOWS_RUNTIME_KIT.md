# Windows Production Runtime Kit

The production Windows hardware contour does not download arbitrary binaries during the release job. Runtime components are runner-owned inputs: they are acquired and reviewed separately, then frozen into an immutable manifest before the hardware workflow is allowed to stage them.

The preferred Windows entry point is:

`scripts/prepare_windows_production_runtime.ps1`

It calls `scripts/build_windows_runtime_kit.py`, stages the resulting lock with `scripts/prepare_sidecars.py`, and immediately runs the fail-closed production verifier. The Python builder itself performs no network access and never downloads runtime components.

## Required component roots

Exactly these component roots are required:

1. `tesseract` — complete Windows Tesseract tree including `tesseract.exe`, `tessdata/rus.traineddata` and `tessdata/eng.traineddata`;
2. `poppler` — complete portable Poppler tree including `pdftotext.exe` and `pdftoppm.exe` plus their DLL/data dependencies;
3. `libreoffice` — complete portable LibreOffice tree including `program/soffice.exe`, `program/soffice.bin`, `program/fundamental.ini` and all runtime dependencies;
4. `sumatrapdf` — reviewed SumatraPDF portable tree including `SumatraPDF.exe`;
5. `7zip` — reviewed portable tree including `7z.exe`/`7zz.exe` and, when `7z.exe` is used, its matching `7z.dll`;
6. `msgconvert` — reviewed MSG conversion runtime and all files needed by its Windows entry point;
7. `llama_cpp` — reviewed llama.cpp runtime including `llama-server.exe`;
8. `semantic_model` — the approved `.gguf` model tree.

There is deliberately no "exclude DLLs" or "copy only the executable" option. Every regular file below each reviewed component root is inventoried and locked. A symlink, Windows junction/reparse indirection, path escape or duplicate target makes the build fail closed.

## Runner-owned layout

A practical layout is:

```text
C:\DokkomplektRuntime\
  source\
    tesseract\...
    poppler\...
    libreoffice\...
    sumatrapdf\...
    7zip\...
    msgconvert\...
    llama_cpp\...
    semantic_model\...
  licenses\...
  runtime-kit.json
  locked\
```

The source trees, license notices and resulting lock stay on the protected Windows runner. Do not commit licensed binaries, models, PFX files or private keys to either GitHub repository.

## Runtime-kit specification

Create `C:\DokkomplektRuntime\runtime-kit.json`. The structure is:

```json
{
  "schema": 1,
  "target": "windows-x86_64",
  "review": {
    "reviewer": "REPLACE_REVIEWER",
    "reviewed_at": "2026-08-09",
    "scope": "complete production Windows portable runtime trees"
  },
  "components": [
    {
      "tool": "tesseract",
      "root": "C:\\DokkomplektRuntime\\source\\tesseract",
      "target_root": "tesseract",
      "version": "REPLACE_VERSION",
      "source_url": "https://REPLACE_REAL_PUBLIC_SOURCE",
      "license": "REPLACE_LICENSE",
      "license_file": "C:\\DokkomplektRuntime\\licenses\\tesseract.txt"
    },
    {
      "tool": "poppler",
      "root": "C:\\DokkomplektRuntime\\source\\poppler",
      "target_root": "poppler",
      "version": "REPLACE_VERSION",
      "source_url": "https://REPLACE_REAL_PUBLIC_SOURCE",
      "license": "REPLACE_LICENSE",
      "license_file": "C:\\DokkomplektRuntime\\licenses\\poppler.txt"
    },
    {
      "tool": "libreoffice",
      "root": "C:\\DokkomplektRuntime\\source\\libreoffice",
      "target_root": "libreoffice",
      "version": "REPLACE_VERSION",
      "source_url": "https://REPLACE_REAL_PUBLIC_SOURCE",
      "license": "REPLACE_LICENSE",
      "license_file": "C:\\DokkomplektRuntime\\licenses\\libreoffice.txt"
    },
    {
      "tool": "sumatrapdf",
      "root": "C:\\DokkomplektRuntime\\source\\sumatrapdf",
      "target_root": "sumatrapdf",
      "version": "REPLACE_VERSION",
      "source_url": "https://REPLACE_REAL_PUBLIC_SOURCE",
      "license": "REPLACE_LICENSE",
      "license_file": "C:\\DokkomplektRuntime\\licenses\\sumatrapdf.txt"
    },
    {
      "tool": "7zip",
      "root": "C:\\DokkomplektRuntime\\source\\7zip",
      "target_root": "7zip",
      "version": "REPLACE_VERSION",
      "source_url": "https://REPLACE_REAL_PUBLIC_SOURCE",
      "license": "REPLACE_LICENSE",
      "license_file": "C:\\DokkomplektRuntime\\licenses\\7zip.txt"
    },
    {
      "tool": "msgconvert",
      "root": "C:\\DokkomplektRuntime\\source\\msgconvert",
      "target_root": "msgconvert",
      "version": "REPLACE_VERSION",
      "source_url": "https://REPLACE_REAL_PUBLIC_SOURCE",
      "license": "REPLACE_LICENSE",
      "license_file": "C:\\DokkomplektRuntime\\licenses\\msgconvert.txt"
    },
    {
      "tool": "llama_cpp",
      "root": "C:\\DokkomplektRuntime\\source\\llama_cpp",
      "target_root": "llama_cpp",
      "version": "REPLACE_VERSION",
      "source_url": "https://REPLACE_REAL_PUBLIC_SOURCE",
      "license": "REPLACE_LICENSE",
      "license_file": "C:\\DokkomplektRuntime\\licenses\\llama-cpp.txt"
    },
    {
      "tool": "semantic_model",
      "root": "C:\\DokkomplektRuntime\\source\\semantic_model",
      "target_root": "semantic_model",
      "version": "REPLACE_MODEL_REVISION",
      "source_url": "https://REPLACE_REAL_PUBLIC_MODEL_SOURCE",
      "license": "REPLACE_MODEL_LICENSE",
      "license_file": "C:\\DokkomplektRuntime\\licenses\\semantic-model.txt"
    }
  ]
}
```

The builder rejects empty values and `REPLACE_*` placeholders. `source_url` must be a real public HTTPS provenance URL or a non-empty reviewed URN accepted by the shared release policy.

## Build, stage and verify in one command

From the exact public source SHA being prepared for production, open PowerShell and run:

```powershell
.\scripts\prepare_windows_production_runtime.ps1 `
  -SpecPath 'C:\DokkomplektRuntime\runtime-kit.json' `
  -OutputDir 'C:\DokkomplektRuntime\locked'
```

The wrapper rejects relative or reparse-point input/output locations, invokes only local Python release scripts, stops on the first non-zero exit code, and prints the final manifest/report SHA-256 values only after the production verifier succeeds.

The output directory contains:

- `runtime-inventory.json` — exact path inventory of every file under all eight reviewed roots;
- `runtime-catalog.json` — local source path + version + origin + license mapping used by the lock generator;
- `windows-x86_64-manifest.json` — immutable SHA-256/provenance lock consumed by hardware validation;
- `RUNTIME_KIT_REPORT.json` — component/file/byte counts and SHA-256 values binding the generated evidence.

The manifest sets `supply_chain_locked=true` only after every file and license notice has been resolved and hashed and the complete inventory has been bound to the distribution review.

## Equivalent explicit commands

For diagnosis, the one-command wrapper is equivalent to:

```powershell
python scripts\build_windows_runtime_kit.py `
  C:\DokkomplektRuntime\runtime-kit.json `
  --output-dir C:\DokkomplektRuntime\locked

python scripts\prepare_sidecars.py `
  C:\DokkomplektRuntime\locked\windows-x86_64-manifest.json `
  --clean

python scripts\assert_offline_runtime_ready.py `
  --target windows-x86_64 `
  --require-semantic-model `
  --require-supply-chain `
  --production
```

The production verifier rejects implausibly small Windows executables, Tesseract language files and GGUF models, requires the complete reviewed LibreOffice/7-Zip structure, and treats `msgconvert` as mandatory.

Only after this passes should the private repository variable `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH` and the runner bootstrap `-SidecarManifestPath` point to:

`C:\DokkomplektRuntime\locked\windows-x86_64-manifest.json`

The subsequent private hardware workflow re-stages and re-verifies this exact runner-owned lock before signing or exercising Word/printer/reboot evidence.
