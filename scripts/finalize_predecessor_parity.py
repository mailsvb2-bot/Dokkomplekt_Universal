from pathlib import Path


def replace(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding='utf-8')
    if old not in text:
        raise SystemExit(f'anchor not found in {path}: {old[:100]!r}')
    p.write_text(text.replace(old, new, 1), encoding='utf-8')


# Restore the predecessor's explicit printer choice through the already-canonical
# backend inventory/preferences API. No print engine or state store is duplicated.
replace(
    'src/components/Workspace.tsx',
    "import { AdditionalMaterialsPanel } from './AdditionalMaterialsPanel';",
    "import { AdditionalMaterialsPanel } from './AdditionalMaterialsPanel';\nimport { PrintPreferencesControl } from './PrintPreferencesControl';",
)
replace(
    'src/components/Workspace.tsx',
    '''            {props.lastOutput.print_items?.length ? (
              <details className="resultCopies">
                <summary>Количество экземпляров для печати</summary>
                <div className="printCopyList">
                  {props.lastOutput.print_items.map((item: GeneratedPrintItem) => (
                    <label key={`${item.document_id}:${item.path}`} className="printCopyRow">
                      <span title={item.path}>{item.label}</span>
                      <input
                        type="number"
                        min={0}
                        max={99}
                        value={props.printCopies[item.document_id] ?? 1}
                        aria-label={`Количество экземпляров для ${item.label}`}
                        onChange={(event) => props.onPrintCopyChange(item.document_id, Number(event.target.value))}
                      />
                      <small>экз.</small>
                    </label>
                  ))}
                </div>
              </details>
            ) : null}''',
    '''            {props.lastOutput.print_items?.length ? (
              <>
                <details className="resultCopies">
                  <summary>Количество экземпляров для печати</summary>
                  <div className="printCopyList">
                    {props.lastOutput.print_items.map((item: GeneratedPrintItem) => (
                      <label key={`${item.document_id}:${item.path}`} className="printCopyRow">
                        <span title={item.path}>{item.label}</span>
                        <input
                          type="number"
                          min={0}
                          max={99}
                          value={props.printCopies[item.document_id] ?? 1}
                          aria-label={`Количество экземпляров для ${item.label}`}
                          onChange={(event) => props.onPrintCopyChange(item.document_id, Number(event.target.value))}
                        />
                        <small>экз.</small>
                      </label>
                    ))}
                  </div>
                </details>
                <PrintPreferencesControl busy={props.busy} />
              </>
            ) : null}''',
)

# Lock a subtle early-donor parser rule: merely mentioning prior treatment in
# narrative prose is not the same as a treatment assignment section.
parity = Path('crates/dokkomplekt-core/tests/donor_exhaustive_parity.rs')
text = parity.read_text(encoding='utf-8')
marker = 'fn donor_narrative_treatment_mention_is_not_an_assignment()'
if marker not in text:
    text += r'''

#[test]
fn donor_narrative_treatment_mention_is_not_an_assignment() {
    let text = concat!(
        "Первичный осмотр\n",
        "Пациент: Иванов Иван Иванович\n",
        "Дата поступления: 10.02.2026\n",
        "Диагноз: F32.1 Депрессивный эпизод\n",
        "Анамнез: ранее проходил лечение амбулаторно, эффект частичный."
    );
    let (case, _) = parse_source_text(text, 2026);
    assert_eq!(case.get("medical.treatment"), None);
}
'''
    parity.write_text(text, encoding='utf-8')

# Make the audit document an explicit closure matrix rather than a broad claim.
doc = Path('docs/DONOR_EXHAUSTIVE_PARITY_2026-08-19.md')
text = doc.read_text(encoding='utf-8')
append_marker = '## Final predecessor closure matrix'
if append_marker not in text:
    text += r'''

## Final predecessor closure matrix

The final audit pass re-opened the executable smoke/regression contours in both pinned donor revisions and mapped late donor salvage work into the same Universal branch. The following are explicit closures, not similarity claims.

| Donor contract | Canonical Universal owner | Closure |
| --- | --- | --- |
| `utf-8-sig -> utf-8 -> cp1251` text intake | `src-tauri/src/universal_intake.rs` | **ported + regression**; plain TXT no longer becomes mojibake |
| historical `laboratory.results`, `LAB_BLOCK`, `labs_block` | `field_aliases` + `field_registry` | **ported + regression** to `medical.labs` |
| explicit user choice `Нет анализов` satisfies required lab block without inventing findings | `popup_profiles` → `workflow_engine` → `popup_engine` | **ported + regression**; one selected-document preflight, no second dialog |
| choose printer vs system default, duplex and tray | existing printer inventory/preferences backend + `PrintPreferencesControl` | **ported + frontend regression**; print engine remains single canonical owner |
| `1985 г.р.` is year-only evidence, not an invented full date | `source_parser` | **salvaged from PR #153 into this branch** |
| exact normalized diagnosis title may resolve offline ICD, but fuzzy lookup must not become zero-touch guessing | `source_parser` + ICD catalogue | **salvaged from PR #153** |
| `Место работы: не работает` is not an employer | `validators` | **salvaged from PR #153** |
| legacy `Место работы / должность` renders from current split/role-scoped facts without persistent duplicate truth | `field_registry` + `template_intelligence` + `medical_semantics` + `document_generation` | **salvaged from PR #153** |
| narrative mention of earlier treatment is not a treatment assignment | canonical source parser | **regression locked** |
| numbered 01–31 date templates | `record_series` / diary calendar | **superseded**; restoring the old table/calendar engine would create a second brain |
| manual old numbered-date template survives auto-selector failure | current programmatic calendar + specialist-owned text library | **semantic intent retained**: program calendar never overwrites specialist text; obsolete numbered-template state is not restored |
| specialist `Texts` folder and individual files | `AdditionalMaterialsPanel` + clause-block storage + `professional_records` | **preserved/expanded**; exact diagnosis → unambiguous compatible → unscoped fallback; incompatible diagnosis content is fail-closed |
| header/footer scanner content | `dokkomplekt-docx` text-bearing Word parts + `scanner_engine` | **preserved/expanded** beyond donor header/footer-only coverage |
| corrupt state must not be silently overwritten | encrypted SQLite snapshot/persistence gate | **superseded by stronger fail-closed recovery**; no plaintext patient settings store is revived |
| patient/service data must not leak to ordinary technical files | encrypted state + privacy policy + service trust-report routing | **preserved/expanded**; service reports stay outside final patient folder and values are hidden unless explicitly enabled |
| create/save without print and create/save/print | generated result + explicit Print action + optional auto-print | **preserved**, now including donor printer selection |
| watched primary starts/raises normal UI without duplicate visible processes | canonical Rust/Tauri watcher + singleton + SHA-bound handoff | **preserved/expanded** by PR #154 |

The donor Python/Tkinter code remains evidence only. No donor parser, calendar, renderer, popup state, watcher state store, or print engine is embedded into Universal.
'''
    doc.write_text(text, encoding='utf-8')
