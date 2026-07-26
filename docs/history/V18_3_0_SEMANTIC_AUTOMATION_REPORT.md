# Dokkomplekt Universal 18.3.0 — semantic automation and offline-first hardening

## Status

Version 18.3.0 is a **source checkpoint**, not a claim that a production Windows installer has been proved. The implementation keeps the product desktop-first and offline-first: ordinary intake, local state, OCR/model invocation, generation, review and manual printing do not require a network connection. Optional distributed coordination is isolated behind an explicitly configured mTLS service.

The governing safety rule remains unchanged:

> The model proposes, Rust validates, and only evidence-backed values may reach a generated document or automatic print path.

## Source requirements retained

The supplied roadmap and all-phases technical specification are retained verbatim in:

- `docs/ROADMAP_SEMANTIC_MAGIC.md`;
- `docs/TECH_SPEC_ALL_PHASES.md`.

This report maps the implemented 18.3.0 checkpoint to those contracts without replacing them.

## 1. Desktop/offline-first boundary

- Removed direct PostgreSQL transport and database credentials from the desktop application.
- Kept local SQLite, content-addressed local/file queue and local execution as the default path.
- Added an optional HTTPS/mTLS queue client. It is not instantiated when the mTLS variables are absent.
- Added a queue service with two server-side backends:
  - SQLite for one service node;
  - PostgreSQL for production/HA, accepted only with certificate-verified TLS settings.
- A configured but unavailable central queue fails closed for distributed publication instead of silently falling back and risking duplicate output.
- Hardened the separate license-server database path: production `NoTls` is accepted only through a local Unix-domain socket; remote clear-text PostgreSQL is rejected before connection.

PostgreSQL therefore remains available where it belongs — behind a server boundary — but is no longer a desktop dependency.

## 2. Offline runtime supply chain

- Added/strengthened hash-locked staging for Tesseract, Russian/English OCR data, Poppler, LibreOffice, SumatraPDF, 7-Zip, llama.cpp and a GGUF model.
- Each staged artifact requires version, source, license notice, SHA-256 and size evidence.
- Production verification additionally requires an exact reviewed portable-tree inventory, including dependent files such as `soffice.bin`, `fundamental.ini` and a matching `7z.dll` when `7z.exe` is used.
- Placeholder or implausibly small executables, OCR data and GGUF files are rejected for production packaging.
- The source archive intentionally excludes third-party binaries and model weights; an offline installer remains blocked until real approved payloads are staged.

## 3. OCR and layout preservation

- Scanned images and image-only PDFs are classified explicitly as `scanned_image` or `scanned_pdf_ocr`.
- Tesseract TSV output is converted into normalized layout records with page, block, line, bounding box, confidence and table-cell information.
- OCR tables no longer collapse into an undifferentiated text string before semantic parsing.
- Layout records reach `SemanticCase.collections["source.layout_items"]` and source metadata blocks.
- Field evidence can inherit OCR page/reference information and cannot claim confidence above the OCR evidence that supports it.
- Loading a new source clears previous `source.*` metadata, preventing layout/evidence leakage between cases.
- The UI shows source type, structural-row count and detected table-row count.
- Added a hash-locked, image-only scanned PDF table golden plus its Tesseract TSV evidence.
- Added a real Poppler → Tesseract smoke verifier; Windows installer/hardware workflows run it against the exact staged offline sidecars before packaging.

## 4. Domain-scoped grounded extraction

- Extraction prompts now receive universal slots plus slots for the active domain/content pack instead of one global cross-domain field list.
- Legal, HR, accounting, education and medical identifiers are no longer unnecessarily mixed in every prompt.
- High-risk detection no longer relies on broad substring matching. Canonical identifiers and semantic suffixes are used, preventing false classification such as treating every `phone_number` as a legal document number.
- Existing literal grounding and multi-pass consensus remain mandatory.

## 5. Ground-truth corpus

- Added a corpus recorder at the correct lifecycle point: after specialist corrections and successful generation, when the final accepted case and actual document set are known.
- The corpus records three distinct views:
  - model proposals;
  - deterministic extraction;
  - final specialist-accepted values.
- Raw source text and raw field values are not stored in the analytical corpus.
- Comparisons use installation-keyed, domain-separated HMAC-SHA-256 rather than plain SHA-256, protecting low-entropy values such as dates and identifiers from dictionary attacks.
- Model evidence is fingerprinted, and provenance/confidence remain available for calibration.
- Corpus collection is opt-in and disabled by default.
- Added local status/count and an atomic anonymized JSON export.
- Removed the last panic shortcut from corpus fingerprinting; cryptographic initialization errors propagate through `Result`.

This closes the central measurement flaw: agreement with another parser is no longer treated as ground truth.

## 6. Document-kit recommendation and learning

- Added conservative document-type clustering and bundle recommendation.
- Recommendations carry source, stable cluster identity, margin and review requirement.
- Proposed kit and actually generated kit are both recorded, enabling precision, recall and exact-match measurement.
- Learned rules follow shadow → measured confirmations → promotion rather than immediate global mutation after one correction.
- File/process locking prevents concurrent cases from losing learning updates.
- The current safe behavior proposes and audits a kit; it does not silently narrow the requested output set without sufficient evidence.

## 7. Calibrated print triage

- Automatic printing is gated by field confidence, evidence, exact template approval and signed calibration evidence.
- Three practical outcomes are represented:
  - automatic print eligibility;
  - generated package requiring focused field review;
  - hold in the review queue.
- Review payloads are encrypted; open marker files do not contain substituted personal values.
- Draft-only starter packs are never eligible for automatic printing.
- Calibration packages must bind the domain/content pack, source corpus, thresholds, target error rate and a separate held-out evaluation.
- Missing, invalid or insufficient calibration does not block document creation; it blocks only automatic printing and routes the package to review.

## 8. Tier-1 form approval

- Starter forms remain honestly marked `draft_only`.
- Added detached Ed25519 approval evidence bound to:
  - named organization and reviewer;
  - jurisdiction and legal basis;
  - review scope and validity period;
  - exact pack version and SHA-256 set of every DOCX revision.
- The tool records approval evidence; it does not pretend to replace legal/domain review.

## 9. Business registry and 1C boundary

- Added a local encrypted counterparty registry indexed by a keyed INN digest.
- Each record is encrypted separately; lookup is O(1) rather than decrypting a monolithic database.
- Legacy storage can be migrated.
- Registry values are validated and require an explicit user confirmation before entering the semantic case.
- Added import/export and a versioned 1C exchange JSON format.
- Live EGRUL/EGRIP enrichment remains an optional adapter requiring an explicitly selected provider and credentials; it is not a dependency of offline operation.

## 10. Parallelism and observability

- Added bounded parallel processing for isolated cases: configurable from 1 to 4 workers, default 2.
- Publication/lease rules remain serialized where shared state is involved.
- Added measured machine runtime, review counts, auto-print decisions, failures and auditable ROI inputs.
- UI wording distinguishes measured runtime from an organization-provided human baseline; it does not present invented “minutes saved” as observed fact.

## 11. Test and release infrastructure

- Tauri command registry and TypeScript API are checked for exact equality; the current surface contains 98 commands.
- Added an isolated Python contract runner. Every historical contract module runs in a fresh process with a per-module timeout and process-tree cleanup, preventing module-global/test-fixture leakage.
- The runner emits `dokkomplekt.python-contract-shards.v1` JSON evidence.
- Strengthened Cargo-gate attestation so it binds Cargo.lock, raw cargo-audit JSON and the exact clean RustSec advisory database commit.
- Removed `.orig` vendor patch artifacts.

## 12. Honest remaining production blockers

The following are **not proved in the current Linux workspace**:

1. Fresh `cargo check`, `clippy`, `cargo test` and RustSec audit for this exact 18.3.0 tree — a real Rust 1.85.1 toolchain is unavailable in the container.
2. Windows hardware E2E: DPAPI, Word COM generation/`PrintOut`, printer spool completion, SumatraPDF, image placeholders, watcher after an actual reboot and installer uninstall/upgrade.
3. Authenticode signing and validation of the final NSIS artifact.
4. A real, licensed and reviewed Windows portable runtime tree and real GGUF model weights.
5. Legal approval of starter forms by a named organization/domain expert.
6. Accuracy/autonomy claims for a domain before a consented corpus and held-out measurements exist.

No report or marker in 18.3.0 overrides these boundaries.
