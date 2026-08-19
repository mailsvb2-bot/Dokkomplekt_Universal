from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8", newline="\n")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one match, got {count}: {old[:120]!r}")
    write(path, text.replace(old, new, 1))


def replace_all_checked(path: str, old: str, new: str, minimum: int = 1) -> None:
    text = read(path)
    count = text.count(old)
    if count < minimum:
        raise SystemExit(f"{path}: expected at least {minimum} matches, got {count}: {old!r}")
    write(path, text.replace(old, new))


# 1. The same final WorkflowPlan must be used both for what the user sees and
# for what the backend accepts. Profile overlays may only decorate/adjust that
# canonical plan; they must never disappear between review and submit.
replace_once(
    "src-tauri/src/subsystems/document_commands.rs",
    """        let plan = build_merged_popup_plan(\n            &doc,\n            &snapshot.semantic_case,\n            &WorkflowFlags {\n                sick_leave_enabled: req.sick_leave_enabled,\n            },\n        );\n        let result = apply_popup_answers(&snapshot.semantic_case, &plan, &req.answers);\n""",
    """        let mut plan = build_merged_popup_plan(\n            &doc,\n            &snapshot.semantic_case,\n            &WorkflowFlags {\n                sick_leave_enabled: req.sick_leave_enabled,\n            },\n        );\n        apply_profile_prompt_overrides(&app, &mut plan)?;\n        let result = apply_popup_answers(&snapshot.semantic_case, &plan, &req.answers);\n""",
)
replace_once(
    "src-tauri/src/subsystems/document_commands.rs",
    """        let plan = plan_workflow_batch(\n            &documents,\n            &snapshot.semantic_case,\n            &WorkflowFlags {\n                sick_leave_enabled: req.sick_leave_enabled,\n            },\n        );\n        let result = apply_popup_answers(&snapshot.semantic_case, &plan, &req.answers);\n""",
    """        let mut plan = plan_workflow_batch(\n            &documents,\n            &snapshot.semantic_case,\n            &WorkflowFlags {\n                sick_leave_enabled: req.sick_leave_enabled,\n            },\n        );\n        apply_profile_prompt_overrides(&app, &mut plan)?;\n        let result = apply_popup_answers(&snapshot.semantic_case, &plan, &req.answers);\n""",
)

# 2. A real uploaded source is part of the user-visible document set in the donor
# products. Preserve that rule for manual generation too, not only watcher mode.
replace_once(
    "src-tauri/src/universal_intake.rs",
    "use std::io::{Cursor, Read as _};",
    "use std::io::{Cursor, Read as _, Write as _};",
)
replace_once(
    "src-tauri/src/universal_intake.rs",
    """    #[cfg(any(target_os = \"windows\", test))]\n    pub fn materialize(&self, workspace: &Path) -> Result<UploadedSourceSession, String> {\n        materialize_sensitive_file(&self.file_name, &self.bytes, workspace)\n    }\n}\n\nimpl Drop for RetainedUploadedSource {\n""",
    """    #[cfg(any(target_os = \"windows\", test))]\n    pub fn materialize(&self, workspace: &Path) -> Result<UploadedSourceSession, String> {\n        materialize_sensitive_file(&self.file_name, &self.bytes, workspace)\n    }\n\n    /// Copy the immutable uploaded source into an atomically published result set.\n    /// The private stage directory is unique per generation, but create_new still\n    /// protects against an accidental filename collision with a generated document.\n    pub fn copy_to_directory(&self, directory: &Path, prefix: &str) -> Result<PathBuf, String> {\n        std::fs::create_dir_all(directory)\n            .map_err(|error| format!(\"Не удалось подготовить папку комплекта: {error}\"))?;\n        let base = safe_file_name(&format!(\"{prefix}{}\", self.file_name));\n        let base_path = Path::new(&base);\n        let stem = base_path\n            .file_stem()\n            .and_then(|value| value.to_str())\n            .unwrap_or(\"Исходный документ\");\n        let extension = base_path.extension().and_then(|value| value.to_str());\n\n        for index in 1..=10_000usize {\n            let name = if index == 1 {\n                base.clone()\n            } else if let Some(extension) = extension {\n                format!(\"{stem} ({index}).{extension}\")\n            } else {\n                format!(\"{stem} ({index})\")\n            };\n            let target = directory.join(name);\n            match std::fs::OpenOptions::new()\n                .write(true)\n                .create_new(true)\n                .open(&target)\n            {\n                Ok(mut file) => {\n                    file.write_all(&self.bytes).map_err(|error| {\n                        format!(\"Не удалось сохранить исходный документ в комплект: {error}\")\n                    })?;\n                    file.sync_all().map_err(|error| {\n                        format!(\"Не удалось зафиксировать исходный документ на диске: {error}\")\n                    })?;\n                    return Ok(target);\n                }\n                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,\n                Err(error) => {\n                    return Err(format!(\n                        \"Не удалось создать копию исходного документа в комплекте: {error}\"\n                    ))\n                }\n            }\n        }\n        Err(\"Не удалось подобрать уникальное имя для исходного документа в комплекте.\".into())\n    }\n}\n\n#[cfg(test)]\nmod retained_uploaded_source_tests {\n    use super::*;\n\n    #[test]\n    fn immutable_uploaded_source_is_copied_with_profession_neutral_name() {\n        let root = std::env::temp_dir().join(format!(\n            \"dokkomplekt-retained-source-{}-{}\",\n            std::process::id(),\n            Uuid::new_v4()\n        ));\n        let source = RetainedUploadedSource::new(\"Договор.docx\", b\"source-bytes\").unwrap();\n        let copied = source.copy_to_directory(&root, \"Исходный - \").unwrap();\n        assert_eq!(\n            copied.file_name().and_then(|value| value.to_str()),\n            Some(\"Исходный - Договор.docx\")\n        );\n        assert_eq!(std::fs::read(&copied).unwrap(), b\"source-bytes\");\n        let _ = std::fs::remove_dir_all(root);\n    }\n}\n\nimpl Drop for RetainedUploadedSource {\n""",
)

replace_once(
    "src-tauri/src/subsystems/document_commands.rs",
    """    let mut counter_reservations = Vec::new();\n    let mut ancillary_warnings = Vec::new();\n    let rendered = (|| -> Result<Vec<PathBuf>, String> {\n""",
    """    let mut counter_reservations = Vec::new();\n    let mut ancillary_warnings = Vec::new();\n    let mut staged_source_copy: Option<PathBuf> = None;\n    let rendered = (|| -> Result<Vec<PathBuf>, String> {\n""",
)
replace_once(
    "src-tauri/src/subsystems/document_commands.rs",
    """        let generated_names = paths\n            .iter()\n            .filter_map(|path| path.file_name())\n            .map(|name| name.to_string_lossy().to_string())\n            .collect::<Vec<_>>();\n        let used_field_ids = documents\n""",
    """        let generated_names = paths\n            .iter()\n            .filter_map(|path| path.file_name())\n            .map(|name| name.to_string_lossy().to_string())\n            .collect::<Vec<_>>();\n        staged_source_copy = {\n            let retained = state\n                .retained_uploaded_source\n                .lock()\n                .map_err(|_| \"uploaded source state lock failed\")?;\n            retained\n                .as_ref()\n                .map(|source| source.copy_to_directory(&stage, \"Исходный - \"))\n                .transpose()?\n        };\n        let used_field_ids = documents\n""",
)
replace_once(
    "src-tauri/src/subsystems/document_commands.rs",
    """    let created_files = verify_published_batch_files(\n        &output_folder,\n        &staged_paths,\n        documents.len(),\n    )?;\n    let created_documents = documents\n""",
    """    let created_files = verify_published_batch_files(\n        &output_folder,\n        &staged_paths,\n        documents.len(),\n    )?;\n    if let Some(staged_source) = staged_source_copy.as_ref() {\n        let source_name = staged_source.file_name().ok_or_else(|| {\n            \"Публикация комплекта не подтверждена: копия исходника не имеет имени файла.\"\n                .to_string()\n        })?;\n        let published_source = output_folder.join(source_name);\n        let metadata = std::fs::metadata(&published_source).map_err(|error| {\n            format!(\n                \"Публикация комплекта не подтверждена: исходный документ отсутствует {}: {error}\",\n                published_source.display()\n            )\n        })?;\n        if !metadata.is_file() || metadata.len() == 0 {\n            return Err(format!(\n                \"Публикация комплекта не подтверждена: копия исходного документа пуста или отсутствует: {}\",\n                published_source.display()\n            ));\n        }\n    }\n    let created_documents = documents\n""",
)

# 3. Real user templates should be understood by default. The existing Rust
# inference is conservative: it edits only a derived copy and only unambiguous
# zones; ambiguous templates remain byte-preserving static copies.
replace_once(
    "src/App.tsx",
    "const [autoInferStaticTemplates, setAutoInferStaticTemplates] = useState(false);",
    "const [autoInferStaticTemplates, setAutoInferStaticTemplates] = useState(true);",
)
replace_all_checked(
    "src/App.tsx",
    "setAutoInferStaticTemplates(false);",
    "setAutoInferStaticTemplates(true);",
    minimum=2,
)

# 4. Frontend regression: first-run static templates must ask Rust to run safe
# inference by default. The test records the actual Tauri payload, not UI text.
replace_once(
    "src/App.test.tsx",
    """function installTemplateMock(staticCopy: boolean) {\n  const calls: string[] = [];\n  __setInvokeForTests(async (name: string) => {\n    calls.push(name);\n""",
    """function installTemplateMock(staticCopy: boolean) {\n  const calls: string[] = [];\n  const confirmRequests: Array<Record<string, unknown> | undefined> = [];\n  __setInvokeForTests(async (name: string, payload?: Record<string, unknown>) => {\n    calls.push(name);\n""",
)
replace_once(
    "src/App.test.tsx",
    """    if (name === 'confirm_template_setup') {\n      return { pack_id: 'default', name: 'Набор', documents: [{ ...sampleDocument, is_static_copy: staticCopy }] } as never;\n    }\n""",
    """    if (name === 'confirm_template_setup') {\n      confirmRequests.push(payload);\n      return { pack_id: 'default', name: 'Набор', documents: [{ ...sampleDocument, is_static_copy: staticCopy }] } as never;\n    }\n""",
)
replace_once(
    "src/App.test.tsx",
    """  return calls;\n}\n""",
    """  return { calls, confirmRequests };\n}\n""",
)
replace_once(
    "src/App.test.tsx",
    """    const calls = installTemplateMock(false);\n    render(<App />);\n    await selectTemplateAndCreateButton();\n    await waitFor(() => expect(screen.getByRole('button', { name: 'Акт выполненных работ' })).toBeTruthy());\n    expect(calls).toContain('confirm_template_setup');\n""",
    """    const { calls } = installTemplateMock(false);\n    render(<App />);\n    await selectTemplateAndCreateButton();\n    await waitFor(() => expect(screen.getByRole('button', { name: 'Акт выполненных работ' })).toBeTruthy());\n    expect(calls).toContain('confirm_template_setup');\n""",
)
replace_once(
    "src/App.test.tsx",
    """    const calls = installTemplateMock(true);\n    render(<App />);\n    await selectTemplateAndCreateButton();\n    await waitFor(() => expect(screen.getByRole('button', { name: 'Акт выполненных работ' })).toBeTruthy());\n    expect(calls).toContain('pick_template_files');\n    expect(calls).toContain('confirm_template_setup');\n""",
    """    const { calls, confirmRequests } = installTemplateMock(true);\n    render(<App />);\n    await selectTemplateAndCreateButton();\n    await waitFor(() => expect(screen.getByRole('button', { name: 'Акт выполненных работ' })).toBeTruthy());\n    expect(calls).toContain('pick_template_files');\n    expect(calls).toContain('confirm_template_setup');\n    expect(confirmRequests.at(-1)).toMatchObject({\n      req: { auto_infer_static_templates: true },\n    });\n""",
)

# The first smoke test does not use the return value, so no change is required.

print("universal donor user-flow patch applied")
