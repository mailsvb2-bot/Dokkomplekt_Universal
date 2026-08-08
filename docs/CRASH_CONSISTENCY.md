# Crash-consistency invariants

Dokkomplekt treats document publication, commercial usage accounting, and template publication as durable state transitions rather than best-effort file writes.

## Live source stability

Watcher intake first captures the source into a private active-session snapshot and proves that the bytes before, during, and after the copy are identical. Recognition, trust-report hashing and optional source-copy publication all use that immutable snapshot. The live source is checked again before publication and after the patient directory becomes visible; a changed source aborts the stale publication and rolls back explicit reservations. Snapshot capture retries only boundedly and fails closed rather than silently processing a file that remains in motion. After a successful publication, destructive archive/delete hygiene first atomically renames the live pathname to a private same-directory `.pending` claim and then verifies the claimed bytes against the processed SHA-256. Destruction and archive receipts are bound to that verified claim rather than the reusable live pathname; a replacement created under the original name is never touched. A mismatched claimed file is recovered under a visible unique name, while stale `.pending` claims left by a process/OS crash are recovered by workspace hygiene after a bounded grace period.

The live-file stability boundary is isolated in `universal_intake/source_snapshot.rs`; archive and web intake keep their own explicit I/O/hash dependencies instead of inheriting them accidentally through the parent module. This keeps the source-snapshot invariant independently testable and prevents future intake refactors from silently coupling live-file verification to unrelated format handlers.

## Template input stability

Every generation run captures each configured template into a private immutable snapshot before planning. The same bytes drive template SHA-256 fingerprints, placeholder extraction, resume fingerprints and DOCX rendering. Live template paths are revalidated before publication and again before commercial commit; replacement of a template during a run aborts stale output and rolls back explicit reservations. Template registration and bulk first-run confirmation likewise analyze, hash and version-copy one captured snapshot rather than reopening a mutable live path between phases.

The template stability boundary is isolated in `template_snapshot.rs`: callers retain the live path only for revalidation and user-facing provenance, while all content-consuming phases use the private snapshot path. Snapshot lifetime is scoped to the operation, so later runs must capture and validate their own template version rather than reusing stale bytes.

## Generated documents

A single generated document is rendered into a hidden same-directory staging file. The user-visible final name is created only after rendering has completed, using atomic create-if-absent publication. A failed or interrupted render must not leave a partial file under a final-looking DOCX name.

## Commercial usage

Usage reservations are persisted before generation. Explicitly observed generation failures roll the reservation back. A reservation left ambiguous by a hard process/OS crash is finalized conservatively rather than refunded automatically, because the crash may have happened after successful publication.

Rollback accounting uses the persisted SQLite reservation row as the source of truth for month, document count, and trial status; caller-provided metadata is never authoritative for a refund.

## Template publication

A candidate `DocumentPack`, desktop state snapshot, and all associated template-version records are committed in one SQLite transaction. The in-memory active pack is replaced only after that transaction commits. Template archive files may be prepared before the transaction, but an archive file alone does not make a template active.

The storage API represents this boundary with a typed `DesktopSnapshotPublication` request, keeping the transaction contract explicit without a long positional-argument interface.

These invariants are covered by storage and Tauri regression tests and are expected to remain fail-closed when a publication primitive or persistence operation cannot guarantee them.