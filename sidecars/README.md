# Bundled sidecars

Dokkomplekt can use Tesseract, Poppler, LibreOffice, SumatraPDF, 7-Zip, msgconvert and a local llama.cpp runtime either from the operating
system or from `src-tauri/resources/tools/<platform-arch>/`.

Release builds must not silently download or trust arbitrary binaries. Prepare a
manifest from approved vendor packages, put an exact SHA-256 beside every file,
and run:

```bash
python scripts/prepare_sidecars.py sidecars/windows-x86_64.json --clean
```

The staging script performs no network requests, rejects path traversal, verifies
every digest and writes `sidecar-status.json`. The resulting directory is bundled
by Tauri. A source archive intentionally contains no third-party executables;
this avoids redistributing unreviewed binaries or incompatible licenses.

For a complete offline Windows installer, include at minimum:

- Tesseract executable, `tessdata/rus.traineddata`, `eng.traineddata`, and its DLLs;
- Poppler `pdftotext.exe`, `pdftoppm.exe`, and their DLLs;
- LibreOffice `program/soffice.exe` and the complete matching portable tree;
- SumatraPDF portable executable for deterministic Windows PDF printing with printer/duplex/tray parameters;
- llama.cpp `llama-server` plus an approved GGUF model when the offline SemanticModel is required;
- optional 7-Zip/msgconvert executables when the corresponding intake formats are enabled;
- license notices, redistribution review and exact upstream versions.

After staging, launch the application and open **Автоматизация → Зависимости**.
Every required tool must be shown as `bundled`, not merely `system`.


The production Windows installer is intentionally fail-closed. Set
`DOKKOMPLEKT_SIDECAR_MANIFEST` to the approved manifest and run
`BUILD_WINDOWS_INSTALLER.bat`. The build calls both `prepare_sidecars.py` and
`assert_offline_runtime_ready.py --require-semantic-model`; a missing, altered or
unapproved binary/model stops the installer build instead of silently reducing
"any document" support.

## Supply-chain-locked production runtime

`sidecar-manifest.example.json` is only a legacy staging example. A production
installer must be built from a generated runtime lock, not from hand-entered
hashes:

```powershell
python scripts/create_runtime_lock.py `
  sidecars/runtime-catalog.reviewed.json `
  --output C:\release-input\windows-x86_64.runtime-lock.json

python scripts/prepare_sidecars.py `
  C:\release-input\windows-x86_64.runtime-lock.json --clean

python scripts/assert_offline_runtime_ready.py `
  --target windows-x86_64 `
  --require-semantic-model `
  --require-supply-chain
```

The reviewed catalog must point to already downloaded local files and an actual
license notice for every artifact. `create_runtime_lock.py` refuses placeholder
versions/origins, computes all SHA-256 digests itself, and requires Tesseract
(rus+eng), Poppler, LibreOffice, SumatraPDF, llama.cpp and a GGUF model. The
staging step copies the license notices into the bundled runtime and the release
verifier re-hashes them. No script in this chain downloads a model or executable.

The source archive does **not** contain third-party binaries or model weights.
That omission is deliberate: redistributing them without a reviewed version,
license and source would turn a security fix into an unreviewed supply-chain
channel. Until a real reviewed runtime lock is supplied, claims such as “любой
скан” and “полностью локальная семантика” are not release-proven.
