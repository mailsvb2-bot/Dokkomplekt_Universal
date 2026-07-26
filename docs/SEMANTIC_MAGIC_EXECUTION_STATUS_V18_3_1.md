# Semantic Magic execution status — v18.3.1 checkpoint

Invariant: the model proposes, Rust verifies, and only proved values may enter generated documents.

## Phase 0 — physical runtime

Implemented in source: hash-verified sidecar staging, fail-closed runtime lock, OCR structure propagation into `SemanticCase`, image-only PDF golden smoke. Not shipped in this source checkpoint: third-party binaries and GGUF weights. Production installer must remain blocked until the runtime bundle is complete and verified.

## Phase 1 — grounded domain extraction

Implemented: domain-scoped schemas, literal source grounding, consensus confidence, provenance/evidence, high-risk consensus rules, encrypted local case state. Grounding must not be weakened to improve coverage.

## Phase 2 — specialist-final corpus

Implemented: opt-in corpus recorder writes after generation, compares model/deterministic observations with the specialist-final accepted case, records the actual kit, and exports privacy-preserving keyed digests rather than raw values.

## Phase 3 — kit selection

Implemented: curated routing/recommendation and evidence-based learned kit rules. A learned rule is not auto-applied until at least 8 observations, 8 clean exact confirmations in a row, and accuracy >= 98%. Any correction resets the clean streak and blocks automatic application.

## Phase 4 — calibrated print triage

Implemented: AutoPrint / ReviewFields / HoldForReview buckets, approved-template requirement, signed calibration package and held-out error evidence. Missing or invalid calibration never blocks document creation; it blocks silent auto-print and sends the bundle to review.

## Remaining external evidence

This source checkpoint is not a Windows production release. It still requires a complete licensed offline runtime, clean Rust/Tauri compilation, current RustSec database audit, Windows Word/print/reboot E2E, NSIS installation tests, and Authenticode evidence.
