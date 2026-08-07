# Crash-consistency invariants

Dokkomplekt treats document publication, commercial usage accounting, and template publication as durable state transitions rather than best-effort file writes.

## Generated documents

A single generated document is rendered into a hidden same-directory staging file. The user-visible final name is created only after rendering has completed, using atomic create-if-absent publication. A failed or interrupted render must not leave a partial file under a final-looking DOCX name.

## Commercial usage

Usage reservations are persisted before generation. Explicitly observed generation failures roll the reservation back. A reservation left ambiguous by a hard process/OS crash is finalized conservatively rather than refunded automatically, because the crash may have happened after successful publication.

Rollback accounting uses the persisted SQLite reservation row as the source of truth for month, document count, and trial status; caller-provided metadata is never authoritative for a refund.

## Template publication

A candidate `DocumentPack`, desktop state snapshot, and all associated template-version records are committed in one SQLite transaction. The in-memory active pack is replaced only after that transaction commits. Template archive files may be prepared before the transaction, but an archive file alone does not make a template active.

These invariants are covered by storage and Tauri regression tests and are expected to remain fail-closed when a publication primitive or persistence operation cannot guarantee them.
