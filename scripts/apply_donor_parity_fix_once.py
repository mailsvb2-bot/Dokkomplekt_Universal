from pathlib import Path
import re
from textwrap import dedent


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one literal match, got {count}: {old[:120]!r}")
    p.write_text(text.replace(old, new, 1), encoding="utf-8")


def sub_once(path: str, pattern: str, repl) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    out, count = re.subn(pattern, repl, text, count=1, flags=re.MULTILINE)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one regex match, got {count}: {pattern[:120]!r}")
    p.write_text(out, encoding="utf-8")


# 1. Domain boundary: medical legacy projection is ephemeral and cannot medicalize other domains.
path = "crates/dokkomplekt-core/src/domains/mod.rs"
marker = "pub mod medical_semantics;\n"
addition = dedent(
    '''

    /// Build an ephemeral case for one document render. Profession-specific legacy
    /// compatibility stays behind the domain boundary and never rewrites stored data.
    pub fn case_for_document_render(
        case: &crate::SemanticCase,
        category: &crate::DomainKind,
        role_id: &str,
    ) -> crate::SemanticCase {
        match category {
            crate::DomainKind::Medical => {
                medical_semantics::case_for_medical_document_render(case, role_id)
            }
            _ => case.clone(),
        }
    }

    #[cfg(test)]
    mod render_case_tests {
        use super::*;
        use crate::{SemanticCase, SemanticValue, ValueSource};

        fn put(case: &mut SemanticCase, field_id: &str, value: &str) {
            case.values.insert(
                field_id.to_string(),
                SemanticValue::new(field_id, value, ValueSource::UserConfirmed, 1.0),
            );
        }

        #[test]
        fn document_render_scopes_medical_role_without_medicalizing_other_domains() {
            let mut case = SemanticCase::default();
            put(
                &mut case,
                medical_semantics::VK_MSE_PROTOCOL_NUMBER,
                "MSE-10",
            );

            let medical =
                case_for_document_render(&case, &crate::DomainKind::Medical, "vk_mse");
            let legal =
                case_for_document_render(&case, &crate::DomainKind::Legal, "vk_mse");

            assert_eq!(medical.get("medical.protocol_number"), Some("MSE-10"));
            assert_eq!(legal.get("medical.protocol_number"), None);
            assert_eq!(case.get("medical.protocol_number"), None);
        }
    }
    '''
).lstrip("\n")
replace_once(path, marker, marker + addition)


# 2. Manual DOCX paths: render and validate the same role-scoped case.
path = "src-tauri/src/subsystems/document_commands.rs"
pattern = r'''(?m)^(?P<i>\s*)let render_result = render_docx_with_assets\(\n(?P=i)    &app,\n(?P=i)    template_snapshot\.path\(\),\n(?P=i)    &reservation\.path,\n(?P=i)    &hydrated\.case,\n(?P=i)    req\.strict,\n(?P=i)    permit\.watermark\.as_deref\(\),\n(?P=i)\);'''


def single_render(match: re.Match[str]) -> str:
    i = match.group("i")
    return (
        f"{i}let render_case = dokkomplekt_core::domains::case_for_document_render(\n"
        f"{i}    &hydrated.case,\n"
        f"{i}    &doc.category,\n"
        f"{i}    &doc.role_id,\n"
        f"{i});\n"
        f"{i}let render_result = render_docx_with_assets(\n"
        f"{i}    &app,\n"
        f"{i}    template_snapshot.path(),\n"
        f"{i}    &reservation.path,\n"
        f"{i}    &render_case,\n"
        f"{i}    req.strict,\n"
        f"{i}    permit.watermark.as_deref(),\n"
        f"{i});"
    )


sub_once(path, pattern, single_render)
replace_once(
    path,
    "        &hydrated.case,\n        &reservation.path,\n    ) {\n",
    "        &render_case,\n        &reservation.path,\n    ) {\n",
)

pattern = r'''(?m)^(?P<i>\s*)if let Err\(error\) = render_docx_with_assets\(\n(?P=i)    &app,\n(?P=i)    template_snapshot\.path\(\),\n(?P=i)    &reservation\.path,\n(?P=i)    &hydrated\.case,\n(?P=i)    req\.strict,\n(?P=i)    permit\.watermark\.as_deref\(\),\n(?P=i)\) \{'''


def batch_render(match: re.Match[str]) -> str:
    i = match.group("i")
    return (
        f"{i}let render_case = dokkomplekt_core::domains::case_for_document_render(\n"
        f"{i}    &hydrated.case,\n"
        f"{i}    &document.category,\n"
        f"{i}    &document.role_id,\n"
        f"{i});\n"
        f"{i}if let Err(error) = render_docx_with_assets(\n"
        f"{i}    &app,\n"
        f"{i}    template_snapshot.path(),\n"
        f"{i}    &reservation.path,\n"
        f"{i}    &render_case,\n"
        f"{i}    req.strict,\n"
        f"{i}    permit.watermark.as_deref(),\n"
        f"{i}) {{"
    )


sub_once(path, pattern, batch_render)
replace_once(
    path,
    "                &hydrated.case,\n                &reservation.path,\n            ) {\n",
    "                &render_case,\n                &reservation.path,\n            ) {\n",
)


# 3. Zero-touch/background generation gets exactly the same role projection and
# final completeness gate as the manual path.
path = "src-tauri/src/subsystems/automation_runtime.rs"
pattern = r'''(?m)^(?P<i>\s*)render_docx_with_assets\(\n(?P=i)    app,\n(?P=i)    template_snapshot\.path\(\),\n(?P=i)    &out_path,\n(?P=i)    &hydrated\.case,\n(?P=i)    true,\n(?P=i)    permit\.watermark\.as_deref\(\),\n(?P=i)\)\n(?P=i)\.map_err\(\|e\| format!\("Не создан «\{\}»: \{e\}", doc\.button_label\)\)\?;\n(?P=i)rerendered_documents = rerendered_documents\.saturating_add\(1\);'''


def zero_touch_render(match: re.Match[str]) -> str:
    i = match.group("i")
    return (
        f"{i}let render_case = dokkomplekt_core::domains::case_for_document_render(\n"
        f"{i}    &hydrated.case,\n"
        f"{i}    &doc.category,\n"
        f"{i}    &doc.role_id,\n"
        f"{i});\n"
        f"{i}render_docx_with_assets(\n"
        f"{i}    app,\n"
        f"{i}    template_snapshot.path(),\n"
        f"{i}    &out_path,\n"
        f"{i}    &render_case,\n"
        f"{i}    true,\n"
        f"{i}    permit.watermark.as_deref(),\n"
        f"{i})\n"
        f"{i}.map_err(|e| format!(\"Не создан «{{}}»: {{e}}\", doc.button_label))?;\n"
        f"{i}ensure_rendered_document_complete(\n"
        f"{i}    doc,\n"
        f"{i}    &template_text,\n"
        f"{i}    &render_case,\n"
        f"{i}    &out_path,\n"
        f"{i})?;\n"
        f"{i}rerendered_documents = rerendered_documents.saturating_add(1);"
    )


sub_once(path, pattern, zero_touch_render)


# 4. Persist the user-selected generic output root. This is universal UI state,
# not a medical/patient concept.
path = "src/lib/appSupport.ts"
replace_once(
    path,
    "export const OUTPUT_PREFS_KEY = 'dokkomplekt.output-folder-parts.v1';\n",
    "export const OUTPUT_PREFS_KEY = 'dokkomplekt.output-folder-parts.v1';\nexport const OUTPUT_ROOT_KEY = 'dokkomplekt.output-root.v1';\n",
)
selection_marker = "export type PendingTemplate = {"
selection_helper = dedent(
    '''
    export function defaultSelectedDocumentIds(documents: DocumentTemplateSpec[]): string[] {
      return documents.filter(shouldSelectDocumentByDefault).map((document) => document.id);
    }

    '''
)
replace_once(path, selection_marker, selection_helper + selection_marker)

output_marker = "export function loadOutputFolderParts(): FolderNamePartDto[] {"
output_helper = dedent(
    '''
    export function loadOutputRoot(): string {
      try {
        const value = localStorage.getItem(OUTPUT_ROOT_KEY)?.trim();
        if (value) return value;
      } catch { /* use generic local default */ }
      return 'output/Готовые документы';
    }

    export function saveOutputRoot(value: string): void {
      const normalized = value.trim();
      if (!normalized) return;
      try { localStorage.setItem(OUTPUT_ROOT_KEY, normalized); } catch { /* storage may be unavailable */ }
    }

    '''
)
replace_once(path, output_marker, output_helper + output_marker)


# 5. Startup, first button creation and workspace reload all use one selection contract.
path = "src/App.tsx"
replace_once(
    path,
    "arrayBufferToBase64, createdPrintItems, cursorMarkedTemplatePath",
    "arrayBufferToBase64, createdPrintItems, defaultSelectedDocumentIds, cursorMarkedTemplatePath",
)
replace_once(
    path,
    "loadAutoPrintPreference, loadOutputFolderParts,\n  loadPrintCopyPreferences",
    "loadAutoPrintPreference, loadOutputFolderParts, loadOutputRoot,\n  loadPrintCopyPreferences",
)
replace_once(
    path,
    "promptToPopupField, readFileBytes,\n  replaceAllLiteral, shouldSelectDocumentByDefault, withPendingTemplateDomain",
    "promptToPopupField, readFileBytes, saveOutputRoot,\n  replaceAllLiteral, withPendingTemplateDomain",
)
replace_once(
    path,
    "  const [outputRoot, setOutputRoot] = useState('output/Готовые документы');\n",
    "  const [outputRoot, setOutputRoot] = useState(loadOutputRoot);\n",
)
replace_once(
    path,
    "setSelectedDocIds(res.pack.documents.filter(shouldSelectDocumentByDefault).map((document) => document.id));",
    "setSelectedDocIds(defaultSelectedDocumentIds(res.pack.documents));",
)
replace_once(
    path,
    "setSelectedDocIds(pack.documents.map((document) => document.id));",
    "setSelectedDocIds(defaultSelectedDocumentIds(pack.documents));",
)
replace_once(
    path,
    "if (res?.pack?.documents) { setDocuments(res.pack.documents); setSelectedDocIds(res.pack.documents.map((document) => document.id)); setStatus(`Рабочий набор загружен: ${res.pack.documents.length} документ(ов).`); }",
    "if (res?.pack?.documents) { setDocuments(res.pack.documents); setSelectedDocIds(defaultSelectedDocumentIds(res.pack.documents)); setStatus(`Рабочий набор загружен: ${res.pack.documents.length} документ(ов).`); }",
)
replace_once(
    path,
    "  useEffect(() => {\n    void updateBackgroundWatcherPreferences(autoPrint, printCopies).catch(() => {\n",
    "  useEffect(() => {\n    saveOutputRoot(outputRoot);\n  }, [outputRoot]);\n\n  useEffect(() => {\n    void updateBackgroundWatcherPreferences(autoPrint, printCopies).catch(() => {\n",
)


# 6. Regression tests for output persistence and shared default button selection.
path = "src/lib/appSupport.selection.test.ts"
replace_once(
    path,
    "import { shouldSelectDocumentByDefault } from './appSupport';\n",
    "import { defaultSelectedDocumentIds, loadOutputRoot, OUTPUT_ROOT_KEY, saveOutputRoot, shouldSelectDocumentByDefault } from './appSupport';\n",
)
marker = "describe('default document selection', () => {"
tests = dedent(
    '''
    describe('output root persistence', () => {
      it('remembers the user-selected generic output folder across restarts', () => {
        localStorage.removeItem(OUTPUT_ROOT_KEY);
        expect(loadOutputRoot()).toBe('output/Готовые документы');
        saveOutputRoot('  D:/Работа/Готовые документы  ');
        expect(loadOutputRoot()).toBe('D:/Работа/Готовые документы');
      });

      it('does not replace a remembered folder with an empty edit', () => {
        localStorage.setItem(OUTPUT_ROOT_KEY, 'C:/Documents/Ready');
        saveOutputRoot('   ');
        expect(loadOutputRoot()).toBe('C:/Documents/Ready');
      });
    });

    '''
)
replace_once(path, marker, tests + marker)
insert_before = "  it('keeps other medical roles selected by default', () => {"
shared_test = dedent(
    '''
      it('applies the same defaults to a whole pack after setup, startup, or reload', () => {
        const documents = [
          document('primary', 'Medical'),
          document('discharge', 'Medical'),
          document('diaries', 'Medical'),
          document('discharge', 'Legal'),
        ];
        expect(defaultSelectedDocumentIds(documents)).toEqual([
          'doc-primary',
          'doc-discharge',
        ]);
      });

    '''
)
replace_once(path, insert_before, shared_test + insert_before)

print("audited donor parity fixes applied")
