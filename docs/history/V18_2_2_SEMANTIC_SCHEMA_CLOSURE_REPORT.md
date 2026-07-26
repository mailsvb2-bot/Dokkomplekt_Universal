# Dokkomplekt Universal 18.2.2 — Semantic schema and release-path closure

## Purpose

This corrective pass closes the highest-risk gaps discovered during the destructive audit of the 18.2.2 source candidate. The changes deliberately prefer a safe stop over producing a convincing but semantically wrong document.

## Corrected classes

1. **Canonical semantic identifiers**
   - Added one compatibility layer for historical ids.
   - `medical.diagnosis_code` is migrated to `medical.icd10`.
   - `organization.*` is migrated to `org.*`.
   - legacy HR and legal ids are migrated to `employee.*` and `contract.*`.
   - persisted legacy values are merged by source priority and confidence instead of becoming a second state.

2. **Alias-aware quality gates**
   - `SemanticCase` now resolves both values and their provenance/confidence through canonical aliases.
   - automation quality checks canonicalize required ids and can no longer silently skip a value because a template/workflow used an older equivalent id.

3. **Role-aware organization extraction**
   - provider/owner and counterparty names, INN and KPP are separated.
   - a checksum-valid INN is no longer assigned to the owner merely because it is the first number found.
   - customer-only documents no longer create a false owner organization through the generic organization heuristic.

4. **Canonical HR and ICD UI paths**
   - ICD selection writes `medical.icd10`.
   - scanner suggestions and template setup use `org.name`, `counterparty.*` and `employee.*`.
   - domain workflows use the same canonical ids as the starter DOCX files.

5. **Content-pack correctness**
   - manifests now contain `referenced_fields`.
   - the validator opens every DOCX/DOCM, reconstructs text split across Word runs, extracts actual placeholders and compares them with the manifest.
   - a populated strict starter/pilot/approved template is rejected if any placeholder is missing from `required_fields` or if the manifest claims a field absent from the document.
   - all 11 starter templates and their public copies were regenerated deterministically; manifests are version 0.3.0.

6. **Fail-closed build path**
   - `СОБРАТЬ_EXE.bat` can no longer bypass the full offline-runtime production builder.
   - project verification creates an isolated Python environment, installs declared dependencies, runs Python contracts and verifies deterministic starter packs.
   - the Windows production builder uses the same Python environment and runs content-pack checks before packaging.

## Verification performed on this tree

- Python regression/source/content-pack contracts: **147/147 passed**.
- Vitest UI/API scenarios: **34/34 passed**.
- TypeScript typecheck: **passed**.
- production frontend build: **passed**.
- starter-pack reproducibility: **11 templates passed**.
- npm audit: **0 vulnerabilities**.
- changed-Rust delimiter/lexical smoke: **passed**.

## Deliberate verification boundary

Rust 1.85.1/cargo is not installed in the execution environment used for this pass. Therefore this report does **not** claim that `cargo fmt`, `cargo check`, `cargo clippy`, `cargo test`, RustSec, Tauri/NSIS packaging or Windows hardware E2E passed. The source remains fail-closed and must pass those existing gates before a production release is approved.
