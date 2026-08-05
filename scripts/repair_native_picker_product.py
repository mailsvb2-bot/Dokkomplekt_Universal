from __future__ import annotations

from pathlib import Path
from textwrap import dedent

ROOT = Path.cwd()


def replace_once(path: str, old: str, new: str, label: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one marker, found {count}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


def main() -> None:
    replace_once(
        "src/App.tsx",
        """    setStatus(`Шаблоны выбраны: ${importedRows.length}. Проверьте названия и нажмите «Создать кнопки».`);
  }

  async function processTemplateFiles(files: File[]) {
""",
        """    setStatus(`Шаблоны выбраны: ${importedRows.length}. Проверьте названия и нажмите «Создать кнопки».`);
  }

  function openTextTemplateSetup() {
    setTemplateText('');
    setButtonLabel('');
    setImportedTemplatePath(null);
    setPendingTemplates([]);
    setDraftPopupFields([]);
    setSetupOpen(true);
    setStatus('Вставьте текст документа, проверьте название кнопки и создайте шаблон.');
  }

  async function processTemplateFiles(files: File[]) {
""",
        "App text-template entry",
    )
    replace_once(
        "src/App.tsx",
        """            onAdd={openTemplateSetup}
            onToggleUtilities={() => setUtilityOpen((value) => !value)}
""",
        """            onAdd={openTemplateSetup}
            onAddFromText={openTextTemplateSetup}
            onToggleUtilities={() => setUtilityOpen((value) => !value)}
""",
        "App DocumentRail props",
    )

    replace_once(
        "src/components/DocumentRail.tsx",
        """  onAdd(): void;
  onToggleUtilities(): void;
""",
        """  onAdd(): void;
  onAddFromText(): void;
  onToggleUtilities(): void;
""",
        "DocumentRail interface",
    )
    replace_once(
        "src/components/DocumentRail.tsx",
        """            <button className="textBtn" onClick={props.onAdd} disabled={props.busy}>Добавить шаблоны</button>
""",
        """            <button className="textBtn" onClick={props.onAdd} disabled={props.busy}>Добавить шаблоны</button>
            <button className="textBtn" onClick={props.onAddFromText} disabled={props.busy}>Создать из текста</button>
""",
        "DocumentRail existing-buttons fallback",
    )
    replace_once(
        "src/components/DocumentRail.tsx",
        """          <button className="primaryBtn full firstRunCreateButtons" onClick={props.onAdd} disabled={props.busy}>Создать свои кнопки</button>
""",
        """          <button className="primaryBtn full firstRunCreateButtons" onClick={props.onAdd} disabled={props.busy}>Создать свои кнопки</button>
          <button className="textBtn firstRunTextTemplate" onClick={props.onAddFromText} disabled={props.busy}>Создать кнопку из текста</button>
""",
        "DocumentRail first-run fallback",
    )

    replace_once(
        "src/App.scenarios.test.tsx",
        """    await click(/Добавить шаблоны/);
    const dialog = await screen.findByRole('dialog', { name: 'Добавление шаблонов' });
""",
        """    const priorTemplateAnalyses = calls.filter((call) => call.command === 'analyze_template_file').length;
    await click(/Добавить шаблоны/);
    const dialog = await screen.findByRole('dialog', { name: 'Добавление шаблонов' });
""",
        "scenario native-picker baseline",
    )
    replace_once(
        "src/App.scenarios.test.tsx",
        "await waitFor(() => expect(calls.filter((c) => c.command === 'analyze_template_file')).toHaveLength(2));",
        "await waitFor(() => expect(calls.filter((c) => c.command === 'analyze_template_file')).toHaveLength(priorTemplateAnalyses + 2));",
        "scenario native-picker count",
    )
    replace_once(
        "src/App.scenarios.test.tsx",
        """    // HTTPS/site/API intake -> parse_web_source
""",
        """    // Text fallback remains explicitly reachable without cancelling the native picker.
    await click(/Создать из текста/);
    const manualDialog = await screen.findByRole('dialog', { name: 'Добавление шаблонов' });
    fireEvent.change(within(manualDialog).getByPlaceholderText('Вставьте текст документа'), { target: { value: 'Договор № {{document.number}}' } });
    fireEvent.click(within(manualDialog).getByRole('button', { name: 'Проверить шаблон' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'analyze_template')).toBe(true));
    fireEvent.click(within(manualDialog).getByRole('button', { name: 'Отмена' }));

    // HTTPS/site/API intake -> parse_web_source
""",
        "scenario text-template fallback",
    )

    replace_once(
        "src/lib/runtimeValidation.test.ts",
        """    expect(Object.keys(COMMAND_RESPONSE_KIND)).toHaveLength(114);
""",
        """    expect(Object.keys(COMMAND_RESPONSE_KIND)).toHaveLength(115);
    expect(COMMAND_RESPONSE_KIND.pick_template_files).toBe('object');
""",
        "runtime command contract",
    )

    replace_once(
        "src/components/DocumentRail.test.tsx",
        """    onAdd: vi.fn(),
    onToggleUtilities: vi.fn(),
""",
        """    onAdd: vi.fn(),
    onAddFromText: vi.fn(),
    onToggleUtilities: vi.fn(),
""",
        "DocumentRail test props",
    )
    replace_once(
        "src/components/DocumentRail.test.tsx",
        dedent("""\
          it('shows one clear first-run action when there are no buttons', () => {
            const onAdd = vi.fn();
            renderRail({ documents: [], activeDocumentId: null, selectedDocumentIds: [], onAdd });
            expect(screen.getByText('Создайте кнопки документов')).toBeTruthy();
            fireEvent.click(screen.getByRole('button', { name: 'Создать свои кнопки' }));
            expect(onAdd).toHaveBeenCalledOnce();
          });
        """),
        dedent("""\
          it('keeps the native picker primary and exposes an explicit text fallback', () => {
            const onAdd = vi.fn();
            const onAddFromText = vi.fn();
            renderRail({ documents: [], activeDocumentId: null, selectedDocumentIds: [], onAdd, onAddFromText });
            expect(screen.getByText('Создайте кнопки документов')).toBeTruthy();
            fireEvent.click(screen.getByRole('button', { name: 'Создать свои кнопки' }));
            fireEvent.click(screen.getByRole('button', { name: 'Создать кнопку из текста' }));
            expect(onAdd).toHaveBeenCalledOnce();
            expect(onAddFromText).toHaveBeenCalledOnce();
          });
        """),
        "DocumentRail first-run test",
    )

    (ROOT / "src/components/TemplateSetupModal.test.tsx").write_text(
        dedent("""\
        import { fireEvent, render, screen, waitFor } from '@testing-library/react';
        import { describe, expect, it, vi } from 'vitest';
        import { TemplateSetupModal } from './TemplateSetupModal';

        const base = {
          templateText: '',
          buttonLabel: '',
          previewTitle: 'Документ',
          pendingTemplates: [],
          draftPopupFields: [],
          onTemplateTextChange: vi.fn(),
          onButtonLabelChange: vi.fn(),
          onDraftPopupFieldsChange: vi.fn(),
          onPendingTemplateLabelChange: vi.fn(),
          onPendingPopupFieldsChange: vi.fn(),
          onMarkupPendingTemplate: vi.fn(async () => undefined),
          onStartGuidedPendingScanner: vi.fn(),
          onAnalyze: vi.fn(),
          onPickFile: vi.fn(),
          onDropFiles: vi.fn(),
          onCancel: vi.fn(),
          onConfirm: vi.fn(),
        };

        describe('TemplateSetupModal', () => {
          it('keeps the first step simple and disables confirmation without input', () => {
            render(<TemplateSetupModal {...base} />);
            expect(screen.getByText('Выберите шаблоны документов')).toBeTruthy();
            expect(screen.getByText(/Каждый DOCX или DOCM сразу станет отдельной кнопкой/)).toBeTruthy();
            expect(screen.getByText('Создать одну кнопку из вставленного текста')).toBeTruthy();
            expect((screen.getByRole('button', { name: 'Создать кнопку' }) as HTMLButtonElement).disabled).toBe(true);
          });

          it('allows a non-empty manual text template without mandatory markup', () => {
            const { rerender } = render(<TemplateSetupModal {...base} />);
            expect((screen.getByRole('button', { name: 'Создать кнопку' }) as HTMLButtonElement).disabled).toBe(true);
            rerender(<TemplateSetupModal {...base} templateText="Пример с Ивановым Иваном" />);
            expect((screen.getByRole('button', { name: 'Создать кнопку' }) as HTMLButtonElement).disabled).toBe(false);
          });

          it('cleans a dangling number mark from the suggested button label', async () => {
            const onPendingTemplateLabelChange = vi.fn();
            render(<TemplateSetupModal {...base} onPendingTemplateLabelChange={onPendingTemplateLabelChange} pendingTemplates={[{
              document_id: 'd1',
              file_name: 'Счёт на оплату.docx',
              button_label: 'Счёт на оплату №',
              extracted_text: 'Счёт № {{document.number}}',
              popup_fields: [],
            }]} />);
            await waitFor(() => expect(onPendingTemplateLabelChange).toHaveBeenCalledWith('d1', 'Счёт на оплату'));
          });

          it('creates every prepared template as a button', () => {
            const onConfirm = vi.fn();
            render(<TemplateSetupModal {...base} onConfirm={onConfirm} pendingTemplates={[
              { document_id: 'd1', file_name: 'Акт.docx', button_label: 'Акт', extracted_text: 'Акт № {{document.number}}', popup_fields: [] },
              { document_id: 'd2', file_name: 'Договор.docx', button_label: 'Договор', extracted_text: 'Договор', popup_fields: [] },
            ]} />);
            expect(screen.getByText('Проверьте названия кнопок')).toBeTruthy();
            expect(screen.getByText('Кнопки готовы к созданию')).toBeTruthy();
            fireEvent.click(screen.getByRole('button', { name: 'Создать кнопки (2)' }));
            expect(onConfirm).toHaveBeenCalledOnce();
          });

          it('allows an unmarked DOCX as an immediately usable static-copy button', () => {
            const onConfirm = vi.fn();
            render(<TemplateSetupModal {...base} onConfirm={onConfirm} pendingTemplates={[{
              document_id: 'd1',
              file_name: 'Пример.docx',
              button_label: 'Пример',
              extracted_text: 'Пример документа',
              popup_fields: [],
            }]} />);
            expect(screen.getByText(/Неразмеченные шаблоны сохранят свою форму и будут доступны сразу/)).toBeTruthy();
            const confirm = screen.getByRole('button', { name: 'Создать кнопки (1)' }) as HTMLButtonElement;
            expect(confirm.disabled).toBe(false);
            fireEvent.click(confirm);
            expect(onConfirm).toHaveBeenCalledOnce();
          });
        });
        """),
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
