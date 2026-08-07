# Crash-consistency invariants

Dokkomplekt treats document publication, commercial usage accounting, and template publication as durable state transitions rather than best-effort file writes.

## Live source stability

Watcher intake first captures the source into a private active-session snapshot and proves that the bytes before, during, and after the copy are identical. Recognition, trust-report hashing and optional source-copy publication all use that immutable snapshot. The live source is checked again before publication and after the patient directory becomes visible; a changed source aborts the stale publication and rolls back explicit reservations. Snapshot capture retries only boundedly and fails closed rather than silently processing a file that remains in motion. After a successful publication, destructive archive/delete hygiene is skipped when the live source no longer matches the processed SHA-256, so a newly replaced source is never deleted as if it were the old case.

## Generated documents

A single generated document is rendered into a hidden same-directory staging file. The user-visible final name is created only after rendering has completed, using atomic create-if-absent publication. A failed or interrupted render must not leave a partial file under a final-looking DOCX name.

## Commercial usage

Usage reservations are persisted before generation. Explicitly observed generation failures roll the reservation back. A reservation left ambiguous by a hard process/OS crash is finalized conservatively rather than refunded automatically, because the crash may have happened after successful publication.

Rollback accounting uses the persisted SQLite reservation row as the source of truth for month, document count, and trial status; caller-provided metadata is never authoritative for a refund.

## Template publication

A candidate `DocumentPack`, desktop state snapshot, and all associated template-version records are committed in one SQLite transaction. The in-memory active pack is replaced only after that transaction commits. Template archive files may be prepared before the transaction, but an archive file alone does not make a template active.

The storage API represents this boundary with a typed `DesktopSnapshotPublication` request, keeping the transaction contract explicit without a long positional-argument interface.

These invariants are covered by storage and Tauri regression tests and are expected to remain fail-closed when a publication primitive or persistence operation cannot guarantee them.
