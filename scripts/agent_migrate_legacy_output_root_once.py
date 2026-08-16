from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one match, found {count}: {old[:140]!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


replace_once(
    "src/lib/appSupport.ts",
    '''export function loadOutputRoot(): string {
  try {
    const value = localStorage.getItem(OUTPUT_ROOT_KEY)?.trim();
    if (value) return value;
  } catch { /* use generic local default */ }
  // First run must ask for a real user-visible destination. A relative
  // application working-directory path is impossible for an end user to locate.
  return '';
}''',
    '''export function loadOutputRoot(): string {
  try {
    const value = localStorage.getItem(OUTPUT_ROOT_KEY)?.trim();
    if (value) {
      // Migrate installations that previously persisted the repository-relative
      // fallback. It is not a user-selected Windows folder and its location
      // depends on the process working directory, so it must never count as a
      // confirmed destination after upgrade.
      const normalized = value.replace(/\\\\/g, '/').replace(/\\/+$/, '');
      if (normalized.toLocaleLowerCase('ru-RU') !== 'output/готовые документы') return value;
    }
  } catch { /* use generic local default */ }
  // First run must ask for a real user-visible destination. A relative
  // application working-directory path is impossible for an end user to locate.
  return '';
}''',
)

replace_once(
    "src/App.tsx",
    '''  async function chooseExistingOutputPolicy(documentIds: string[]): Promise<'version' | 'replace_with_backup' | null> {
    const labels = documentIds.map(id => documents.find(document => document.id === id)?.button_label).filter((value): value is string => Boolean(value));
    const planned = await run('get_output_plan', () => getOutputPlan(outputRoot.trim() || 'output/Готовые документы', folderParts, labels));''',
    '''  async function chooseExistingOutputPolicy(documentIds: string[]): Promise<'version' | 'replace_with_backup' | null> {
    const explicitOutputRoot = outputRoot.trim();
    if (!explicitOutputRoot) {
      setStatus('Сначала выберите папку готовых документов. Ничего не создано.');
      setFolderNamingConfirmed(false);
      return null;
    }
    const labels = documentIds.map(id => documents.find(document => document.id === id)?.button_label).filter((value): value is string => Boolean(value));
    const planned = await run('get_output_plan', () => getOutputPlan(explicitOutputRoot, folderParts, labels));''',
)

replace_once(
    "src/App.tsx",
    '''    const res = await run('render_docx_batch', () => renderDocxBatch(documentIds, outputRoot.trim() || 'output/Готовые документы', folderParts, true, existingOutputPolicy));''',
    '''    const explicitOutputRoot = outputRoot.trim();
    if (!explicitOutputRoot) {
      setStatus('Сначала выберите папку готовых документов. Ничего не создано.');
      setFolderNamingConfirmed(false);
      return;
    }
    const res = await run('render_docx_batch', () => renderDocxBatch(documentIds, explicitOutputRoot, folderParts, true, existingOutputPolicy));''',
)

replace_once(
    "src/App.tsx",
    '''      {!folderNamingConfirmed && (''',
    '''      {(!folderNamingConfirmed || !outputRoot.trim()) && (''',
)

replace_once(
    "src/lib/appSupport.selection.test.ts",
    '''  it('does not replace a remembered folder with an empty edit', () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'C:/Documents/Ready');
    saveOutputRoot('   ');
    expect(loadOutputRoot()).toBe('C:/Documents/Ready');
  });''',
    '''  it('does not replace a remembered folder with an empty edit', () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'C:/Documents/Ready');
    saveOutputRoot('   ');
    expect(loadOutputRoot()).toBe('C:/Documents/Ready');
  });

  it('migrates the old repository-relative fallback back to an unconfigured state', () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'output/Готовые документы');
    expect(loadOutputRoot()).toBe('');
    localStorage.setItem(OUTPUT_ROOT_KEY, 'output\\\\Готовые документы\\\\');
    expect(loadOutputRoot()).toBe('');
  });''',
)

replace_once(
    "tests/test_donor_diary_output_parity.py",
    '''    assert "currentRoot={outputRoot}" in app
    assert "onPickRoot={() => void chooseFolder" in app
    assert "Создано документов:" in workspace''',
    '''    assert "currentRoot={outputRoot}" in app
    assert "onPickRoot={() => void chooseFolder" in app
    assert "(!folderNamingConfirmed || !outputRoot.trim())" in app
    assert "outputRoot.trim() || 'output/Готовые документы'" not in app
    assert "Сначала выберите папку готовых документов. Ничего не создано." in app
    assert "Создано документов:" in workspace''',
)

print("legacy output destination migration applied")
