# Exhaustive donor parity audit — 2026-08-19

This document is an executable-migration companion for the two predecessor repositories that grew into Dokkomplekt Universal. It records donor behavior that must survive the rewrite without importing a second runtime or reviving obsolete engines.

## Audited donor revisions

- `mailsvb2-bot/diary-filler` — `cee7d863e21fdf5c9d9a4d8d88732e9a10819ec7`
- `mailsvb2-bot/Dokkomplekt` — `b4bd25de24e5fd7c5c3374bd9928ce87fa5fdcbd`
- Universal baseline for this audit — `9b62b874e1ff68aaa18c6267f9923a77dbf2eec1`

The donor repositories are specifications and regression evidence, not runtimes to embed. Canonical ownership remains Rust domain core + Tauri shell + thin TypeScript UI.

## Migration rule

For every donor behavior, the result must be one of:

1. **preserved in the canonical Universal engine**;
2. **ported into that engine with regression evidence**;
3. **explicitly superseded** because Universal has a stronger single canonical implementation;
4. **intentionally rejected** because restoring it would recreate a second brain, duplicate calendar/template renderer, hidden patient-data store, or unsafe fail-open behavior.

“Similar behavior exists” is not sufficient. The donor regression contract must map to concrete Universal code/tests.

## User workflow contracts retained

### Primary source and intake

- DOCX/DOCM and supported legacy Word intake remain accepted through the canonical intake/conversion boundary.
- Drag-and-drop and manual selection converge on the same source intake.
- Russian Windows-1251 TXT is normalized before semantic intake; valid UTF-8 and binary documents are not rewritten.
- Candidate identity includes path/size/mtime/ctime so file replacement does not reuse a stale signature.
- Intake never marks an interaction successful merely because a popup opened; success/ignore semantics belong to the canonical queue/watch workflow.
- Corrupt or unstable files fail closed and retry only through bounded watcher policy.
- No patient history is persisted as convenience telemetry/settings.

### Primary-document parsing

- Admission date is contextual and must not be stolen from a birth-date marker.
- Same-line admission/discharge facts remain independent.
- Doctor-confirmed/manual values outrank extracted values.
- FIO demographic tails such as `, 1975 г.р.` are removed from the canonical name while initials remain valid.
- Diagnosis, treatment, complaints, anamneses, status, labs, work/position and profile fields are semantic values, not raw template-copy content.
- Template instructions/service phrases are rejected from medical content.

### Custom templates and Block 03 buttons

- The doctor/user owns the templates.
- A button is created from the user's template and keeps stable semantic identity/role/template mapping.
- Renaming a visible label must not change semantic role or template identity.
- Removing a button must not delete the source template.
- Unrelated maintenance must not erase custom buttons.
- Template structure may be learned; donor/training patient content must never become generated patient content.

### Popup / preflight

- Missing and uncertain fields are requested together through one canonical preflight path.
- Cancel before confirmation must not render a partial document set.
- Blank UI values must not overwrite already extracted meaningful values.
- Doctor-confirmed values have highest priority.
- Diagnosis fields use the medical ICD-10 capability regardless of the dynamic field spelling/role.
- Role-scoped commission/protocol/work-position fields remain separate instead of collapsing into generic aliases.

### Diaries

Canonical owner: `record_series` + `diary_engine` + `professional_records` + medical profile adapters.

Retained donor contracts:

- first ordinary diary is D0+1;
- discharge is a hard upper boundary;
- final discharge entry can replace an ordinary row on the discharge date;
- treating physician + department-head signature blocks are retained;
- specialist-owned diary text remains the source of truth;
- diagnosis-scoped diary text does not leak across incompatible diagnoses;
- grammatical gender adaptation remains medical-profile-only and supports Russian/Polish donor forms;
- confirmed cadence outranks inferred/profile cadence;
- default daily calendar needs no numbered 01–31 template;
- clinical schedule is `+1,+2,+3,+7,+10,+14,+17,+21...` and then twice weekly;
- intraday menu semantics are exact: `2→240m, 3→60m, 4→30m, 5→15m, 6→5m`;
- arbitrary safe custom minute rhythm is accepted (for example `45 минут`);
- custom hour interval lists are preserved (for example `1,2,4,8`);
- negative interval input is rejected;
- fixed clock-time cadence and day-offset + intraday cadence stay canonical in `record_series`;
- dynamic epicrisis cadence stays anchored to each +10-day point, shifts to the donor working-day rule, is capped, and never reaches/overruns discharge.

### Labs

- EPI/epicrisis text must never be silently reused as labs.
- “Нет анализов” is an explicit semantic value, not invented normal findings.
- Old `ОАК`/`ОАМ` placeholders must never produce fabricated `в норме` content.
- Unrelated files are not accepted as laboratory results merely because they are text documents.
- Lab aliases resolve to one canonical medical labs field.

### ICD-10

- Complete bundled non-F detail catalog is retained, not only chapter/block headings.
- Curated psychiatric F wording keeps priority where applicable.
- Detailed somatic codes remain searchable offline.
- Duplicate-code regressions are forbidden.

### Polish donor scenario

The Universal source parser already contains the donor Polish labels (`Pacjent/Pacjentka`, `Nr historii choroby`, `Data urodzenia`, `Data przyjęcia`, `Data wypisu`, `Rozpoznanie`, `Leczenie`, `Zalecenia`, anamnesis/status/work/signature labels), Polish word-date parsing and Polish diary gender pairs.

The audit found one remaining activation risk: the medical-profile gate in `parse_source_text` is currently keyed primarily by Russian medical marker words. A Polish-only source must therefore be locked by an end-to-end regression before this audit can call the Polish path complete. This is a concrete follow-up in this branch, not a reason to introduce a separate Polish parser.

### Output and publication

- Patient/result folder naming follows persisted user-confirmed naming strategy.
- Generated documents preserve the user's Word template structure instead of flattening to plain text.
- Successful generation does not require an informational success popup; opening/surfacing the output can remain the completion UX.
- Printing remains a publication action, not part of semantic extraction.

## Explicitly superseded — do not resurrect

The following donor implementation details are intentionally not copied:

- Python/Tkinter application runtime inside Universal;
- old 01–31 numbered diary date-template engine as a parallel calendar brain;
- stale table-based diary inference when the program calendar is canonical;
- duplicate medical-only renderer beside the universal renderer;
- duplicate popup/scanner state stores;
- bundled narrow-profile patient-content templates as a source of generated content;
- pathful patient history in settings;
- unconditional PowerShell/VBS flashing helpers;
- silent fail-open defaults for missing required medical facts.

Their useful behavior is preserved in the canonical Universal owner instead.

## Evidence added by this audit

`crates/dokkomplekt-core/tests/donor_exhaustive_parity.rs` locks late donor contracts that were previously spread across many Python regressions: clinical calendar offsets, minute-menu semantics, custom minute/hour choices, negative-input rejection, compact/Russian/Polish dates, same-line source parsing/FIO sanitization, dynamic epicrisis cadence and representative full ICD-10 rows.

The compatibility schedule parser is aligned with the same donor menu/custom interval semantics rather than having a second interpretation.

## Remaining acceptance work for this branch

Before merge:

- prove the new parity suite on exact branch SHA;
- add an end-to-end Polish-only medical source regression and close the medical-marker activation gap if it fails;
- compare donor install/update intake handoff with the current Tauri watcher lifecycle and port only any behavior not already structurally superseded;
- verify custom-button diary flow reaches the same cadence fields as checkbox/profile generation;
- run complete canonical Quality Gate, Windows preview/installer and macOS smoke; no bypass/merge on partial green.

Production issue #5 remains a separate physical signing/hardware acceptance boundary and must not be falsely closed by donor code migration.
