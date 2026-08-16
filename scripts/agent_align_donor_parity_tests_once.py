from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected 1 match, found {count}: {old!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")

# Keep the old test-visible heading wording while preserving the new explicit
# result folder and created-file list. This remains unambiguous to the user and
# avoids changing a large unrelated scenario test solely for copy wording.
replace_once(
    "src/components/Workspace.tsx",
    '<h2>Документы созданы: {props.lastOutput.files.length}</h2>',
    '<h2>Создано документов: {props.lastOutput.files.length}</h2>',
)
replace_once(
    "tests/test_donor_diary_output_parity.py",
    'assert "Документы созданы:" in workspace',
    'assert "Создано документов:" in workspace',
)

# Fresh install deliberately has no implicit filesystem destination now.
replace_once(
    "src/lib/appSupport.selection.test.ts",
    "expect(loadOutputRoot()).toBe('output/Готовые документы');",
    "expect(loadOutputRoot()).toBe('');",
)

# The onboarding contract now includes the real parent output directory.
path = Path("src/components/FolderNamingOnboarding.test.tsx")
text = path.read_text(encoding="utf-8")
text = text.replace(
    "    const onConfirm = vi.fn();\n    render(<FolderNamingOnboarding currentParts={['DocumentNumber', 'DocumentDate']} onConfirm={onConfirm} />);",
    "    const onConfirm = vi.fn();\n    const onPickRoot = vi.fn();\n    render(<FolderNamingOnboarding currentRoot=\"D:/Работа/Готовые документы\" currentParts={['DocumentNumber', 'DocumentDate']} onPickRoot={onPickRoot} onConfirm={onConfirm} />);",
)
text = text.replace(
    "    expect(screen.queryByText(/папк.*пациент/i)).toBeNull();",
    "    expect(screen.queryByText(/папк.*пациент/i)).toBeNull();\n    expect(screen.getByText('D:/Работа/Готовые документы')).toBeTruthy();\n    expect(screen.getByRole('button', { name: 'Выбрать папку на компьютере' })).toBeTruthy();",
)
text = text.replace(
    "    fireEvent.click(screen.getByRole('button', { name: 'Сохранить правило' }));",
    "    fireEvent.click(screen.getByRole('button', { name: 'Сохранить папку и правило' }));",
)
if "currentRoot=\"D:/Работа/Готовые документы\"" not in text or "Сохранить папку и правило" not in text:
    raise SystemExit("FolderNamingOnboarding test alignment failed")
path.write_text(text, encoding="utf-8")

print("aligned donor parity tests")
