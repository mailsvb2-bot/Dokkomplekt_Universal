import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { DocumentTemplateSpec } from '../lib/types';
import { DocumentRail } from './DocumentRail';

const document: DocumentTemplateSpec = {
  id: 'discharge',
  button_label: 'Выписной эпикриз',
  template_path: 'Выписной эпикриз.docx',
  category: 'Medical',
  role_id: 'medical.discharge',
  required_fields: [],
  placeholders: [],
  is_static_copy: false,
};

const diaryDocument: DocumentTemplateSpec = {
  ...document,
  id: 'diaries',
  button_label: 'Дневники наблюдения',
  template_path: 'Дневники наблюдения.docx',
  role_id: 'medical.diaries',
};

function buildProps(overrides: Partial<Parameters<typeof DocumentRail>[0]> = {}): Parameters<typeof DocumentRail>[0] {
  return {
    documents: [document],
    activeDocumentId: document.id,
    selectedDocumentIds: [document.id],
    busy: false,
    printCopies: { [document.id]: 1 },
    extraRulesEnabled: false,
    onExtraRulesChange: vi.fn(),
    onSelect: vi.fn(),
    onToggleSelected: vi.fn(),
    onPrintCopiesChange: vi.fn(),
    onSelectAll: vi.fn(),
    onClearSelected: vi.fn(),
    onGenerateSelected: vi.fn(),
    onRename: vi.fn(),
    onConfigurePopups: vi.fn(),
    onScanTemplate: vi.fn(),
    onApprove: vi.fn(),
    onRemove: vi.fn(),
    onAdd: vi.fn(),
    onToggleUtilities: vi.fn(),
    ...overrides,
  };
}

function renderRail(overrides: Partial<Parameters<typeof DocumentRail>[0]> = {}) {
  const props = buildProps(overrides);
  const view = render(<DocumentRail {...props} />);
  return { props, ...view };
}

describe('DocumentRail', () => {
  it('shows one clear first-run action when there are no buttons', () => {
    const onAdd = vi.fn();
    renderRail({ documents: [], activeDocumentId: null, selectedDocumentIds: [], onAdd });
    expect(screen.getByText('Создайте кнопки документов')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Создать свои кнопки' }));
    expect(onAdd).toHaveBeenCalledOnce();
  });

  it('uses the simple complete-package wording and keeps add-buttons visible', () => {
    const { props } = renderRail();
    fireEvent.click(screen.getByRole('button', { name: 'Создать документы (1)' }));
    fireEvent.click(screen.getByRole('button', { name: 'Добавить шаблоны' }));
    expect(props.onGenerateSelected).toHaveBeenCalledOnce();
    expect(props.onAdd).toHaveBeenCalledOnce();
    expect(screen.getByText('в комплекте')).toBeTruthy();
  });

  it('deselects only newly added templates that arrived auto-selected', async () => {
    const onToggleSelected = vi.fn();
    const initialProps = buildProps({
      documents: [document],
      selectedDocumentIds: [],
      onToggleSelected,
    });
    const { rerender } = render(<DocumentRail {...initialProps} />);

    const nextProps = buildProps({
      documents: [document, diaryDocument],
      selectedDocumentIds: [document.id, diaryDocument.id],
      onToggleSelected,
      printCopies: { [document.id]: 1, [diaryDocument.id]: 1 },
    });
    rerender(<DocumentRail {...nextProps} />);

    await waitFor(() => expect(onToggleSelected).toHaveBeenCalledOnce());
    expect(onToggleSelected).toHaveBeenCalledWith(diaryDocument.id);
    expect(onToggleSelected).not.toHaveBeenCalledWith(document.id);
  });

  it('does not reset selection when the same document ids are refreshed', async () => {
    const onToggleSelected = vi.fn();
    const firstProps = buildProps({ onToggleSelected });
    const { rerender } = render(<DocumentRail {...firstProps} />);
    await waitFor(() => expect(onToggleSelected).toHaveBeenCalledOnce());
    onToggleSelected.mockClear();

    const refreshedProps = buildProps({
      documents: [{ ...document }],
      selectedDocumentIds: [document.id],
      onToggleSelected,
    });
    rerender(<DocumentRail {...refreshedProps} />);

    await waitFor(() => expect(onToggleSelected).not.toHaveBeenCalled());
  });

  it('disables generation until at least one document is selected', () => {
    renderRail({ selectedDocumentIds: [] });
    expect((screen.getByRole('button', { name: 'Выберите документы' }) as HTMLButtonElement).disabled).toBe(true);
  });
});
