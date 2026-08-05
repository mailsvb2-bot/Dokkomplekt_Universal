from __future__ import annotations

import runpy
from pathlib import Path

ROOT = Path.cwd()
TEST_PATH = ROOT / "src/components/DocumentRail.test.tsx"


def main() -> None:
    text = TEST_PATH.read_text(encoding="utf-8")
    indented = """  it('shows one clear first-run action when there are no buttons', () => {
    const onAdd = vi.fn();
    renderRail({ documents: [], activeDocumentId: null, selectedDocumentIds: [], onAdd });
    expect(screen.getByText('Создайте кнопки документов')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Создать свои кнопки' }));
    expect(onAdd).toHaveBeenCalledOnce();
  });
"""
    normalized = """it('shows one clear first-run action when there are no buttons', () => {
  const onAdd = vi.fn();
  renderRail({ documents: [], activeDocumentId: null, selectedDocumentIds: [], onAdd });
  expect(screen.getByText('Создайте кнопки документов')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Создать свои кнопки' }));
  expect(onAdd).toHaveBeenCalledOnce();
});
"""
    count = text.count(indented)
    if count != 1:
        raise RuntimeError(f"DocumentRail indentation marker: expected one block, found {count}")
    TEST_PATH.write_text(text.replace(indented, normalized, 1), encoding="utf-8")
    runpy.run_path(str(Path(__file__).with_name("repair_native_picker_product.py")), run_name="__main__")


if __name__ == "__main__":
    main()
