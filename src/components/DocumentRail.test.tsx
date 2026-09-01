import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { DocumentTemplateSpec } from '../lib/types';
import { DocumentRail } from './DocumentRail';

const document: DocumentTemplateSpec = {
  id: 'discharge',
  button_label: 'Выписной эпикриз',
  template_path: 'Выписной эпикриз.docx',
  category: 'Medical',
  role_id: 'discharge',
  required_fields: [],
  placeholders: [],
  is_static_copy: false,
};

const diaryDocument: DocumentTemplateSpec = {
  ...document,
  id: 'diaries',
  button_label: 'Дневники наблюдения',
  template_path: 'Дневники наблюдения.docx',
  role_id: 'diaries',
};

function buildProps(overrides: Partial<Parameters<typeof DocumentRail>[0]> = {}): Parameters<typeof DocumentRail>[0] {
  return {
    documents: [document],
    activeDocumentId: document.id,
    selectedDocumentIds: [document.id],
    busy: false,
    workspaceStateReady: true,
    printCopies: { [document.id]: 1 },
    onSelect: vi.fn(),
    onToggleSelected: vi.fn(),
    onPrintCopiesChange: vi.fn(),
    onSelectAll: vi.fn(),
    onClearSelected: vi.fn(),
    onRename: vi.fn(),
    onConfigurePopups: vi.fn(),
    onScanTemplate: vi.fn(),
    onApprove: vi.fn(),
    onRemove: vi.fn(),
    onAdd: vi.fn(),
    onAddFromText: vi.fn(),
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
  it('never exposes first-run actions before durable workspace bootstrap finishes', () => {
    renderRail({ documents: [], activeDocumentId: null, selectedDocumentIds: [], workspaceStateReady: false });
    expect(screen.getByRole('status', { name: 'Загрузка рабочего набора' })).toBeTruthy();
    expect(screen.getByText('Загружаем сохранённые кнопки…')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Создать свои кнопки' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Создать кнопку из текста' })).toBeNull();
  });

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

  it('keeps document selection and button management separate from generation', () => {
    const { props } = renderRail();
    fireEvent.click(screen.getByRole('button', { name: 'Добавить шаблоны' }));
    expect(props.onAdd).toHaveBeenCalledOnce();
    expect(screen.getByText('в комплекте')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Создать документы/ })).toBeNull();
  });

  it('does not silently deselect newly added templates', () => {
    const onToggleSelected = vi.fn();
    const { rerender } = render(<DocumentRail {...buildProps({ documents: [document], selectedDocumentIds: [document.id], onToggleSelected })} />);
    rerender(<DocumentRail {...buildProps({
      documents: [document, diaryDocument],
      selectedDocumentIds: [document.id, diaryDocument.id],
      onToggleSelected,
      printCopies: { [document.id]: 1, [diaryDocument.id]: 1 },
    })} />);
    expect(onToggleSelected).not.toHaveBeenCalled();
    expect(screen.getAllByText('в комплекте')).toHaveLength(2);
  });

  it('uses checkboxes only for explicit user selection changes', () => {
    const onToggleSelected = vi.fn();
    renderRail({ onToggleSelected });
    fireEvent.click(screen.getByRole('checkbox', { name: 'Добавить Выписной эпикриз в комплект' }));
    expect(onToggleSelected).toHaveBeenCalledWith(document.id);
  });
});
