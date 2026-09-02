import { fireEvent, render, screen } from '@testing-library/react';
import { useState, type ComponentProps } from 'react';
import { describe, expect, it } from 'vitest';
import type { PromptSpec, WorkflowPlan } from '../lib/types';
import { Workspace } from './Workspace';

const sickLeave: PromptSpec = {
  field_id: 'medical.diary_sick_leave_epicrisis', title: 'Лечится по больничному листу?',
  required: true, input_kind: 'yes_no', ask_mode: 'always', options: ['Нет', 'Да'],
};
const correction: PromptSpec = {
  field_id: 'medical.diary_treatment_correction', title: 'Коррекция лечения', required: false,
  input_kind: 'long_text', ask_mode: 'confirm', linked_to: sickLeave.field_id,
  current_value: 'Лекарства принимает согласно назначениям.',
};
const plan: WorkflowPlan = {
  document_id: 'diaries', prompts: [sickLeave, correction], blocked: false, block_reasons: [],
};

const noop = () => undefined;
function Harness() {
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [skippedAnswers, setSkippedAnswers] = useState<Record<string, boolean>>({});
  const props = {
    busy: false, documents: [], selectedDocumentIds: ['diaries'], watchFolder: '', intakeSource: '',
    intakeResult: null, lastOutput: null, autoPrint: false, printCopies: {}, sourceText: '',
    sourceFileName: 'patient.docx', sourceFilePath: 'C:/patient.docx', webSourceUrl: '',
    intakeCapabilities: [], scannerField: '', scannerText: '', parsed: { title: 'patient', count: 4, warnings: [] },
    modelOutput: '', semantic: null, plan, planLoading: false, selectedDocumentCount: 1,
    activeDocumentLabel: 'Дневники', showSickLeaveOption: false, sickLeaveEnabled: false,
    answers, skippedAnswers, preview: null, setAnswers, setSkippedAnswers,
    setIntakeSource: noop, setAutoPrint: noop, setSourceText: noop, setSourceFileName: noop,
    setWebSourceUrl: noop, setScannerField: noop, setScannerText: noop, setModelOutput: noop,
    onPickWatchFolder: noop, onInstallWatcher: noop, onUninstallWatcher: noop, onSickLeaveChange: noop,
    onRunZeroTouch: noop, onOpenLastOutput: noop, onPrintLastOutput: noop, onExportLastOutputPdf: noop,
    onExportLastOutputPdfa: noop, onExportLastOutputKedo: noop, onPickSourceFile: noop,
    onDropSourceFile: noop, onLoadWebSource: noop, onResetCase: noop, onParseSource: noop,
    onStartGuidedSourceScanner: noop, onReportSemanticError: noop, onApplyScannerSelection: noop,
    onApplyScannerAndQuestion: noop, onPrintCopyChange: noop, onUnderstand: noop, onPinField: noop,
    onPreview: noop, onCreateSelected: noop,
  } as ComponentProps<typeof Workspace>;
  return <Workspace {...props} />;
}
describe('Workspace linked medical diary prompts', () => {
  it('uses the same Yes/No visibility contract as final preflight', () => {
    render(<Harness />);
    expect(screen.getByText('Лечится по больничному листу?')).toBeTruthy();
    expect(screen.queryByText('Коррекция лечения')).toBeNull();

    fireEvent.change(screen.getByLabelText('Лечится по больничному листу?*'), { target: { value: 'Да' } });
    expect(screen.getByText('Коррекция лечения')).toBeTruthy();
    expect((screen.getByLabelText('Коррекция лечения') as HTMLTextAreaElement).value)
      .toBe('Лекарства принимает согласно назначениям.');

    fireEvent.change(screen.getByLabelText('Лечится по больничному листу?*'), { target: { value: 'Нет' } });
    expect(screen.queryByText('Коррекция лечения')).toBeNull();
  });
});
