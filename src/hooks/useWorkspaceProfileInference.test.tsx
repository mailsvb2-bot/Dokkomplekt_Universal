import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { prepareTemplateSetup } from '../lib/api';
import type { PendingTemplate } from '../lib/appSupport';
import type { TemplateConfirmationRowDto, WorkspaceProfileInference } from '../lib/types';
import { useWorkspaceProfileInference } from './useWorkspaceProfileInference';

vi.mock('../lib/api', () => ({ prepareTemplateSetup: vi.fn() }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });
  return { promise, resolve, reject };
}

function pending(documentId: string): PendingTemplate {
  return {
    document_id: documentId,
    template_path: `${documentId}.docx`,
    extracted_text: documentId,
    file_name: `${documentId}.docx`,
    button_label: documentId,
    popup_fields: [],
    domain_override: null,
  };
}

function inference(domain: 'Medical' | 'Legal'): WorkspaceProfileInference {
  return {
    suggested_domain: domain,
    confidence: 0.9,
    level: 'high',
    auto_apply: true,
    mixed_domains: false,
    domain_scores: {},
    evidence: [],
    reasons: [],
  };
}

function response(value: WorkspaceProfileInference): TemplateConfirmationRowDto[] {
  return [{
    document_id: 'row',
    template_path: 'row.docx',
    detected_title: 'Row',
    suggested_button_label: 'Row',
    editable_button_label: 'Row',
    role_id: 'unknown',
    is_static_copy: true,
    analysis: {},
    workspace_inference: value,
  }];
}

describe('useWorkspaceProfileInference', () => {
  beforeEach(() => vi.clearAllMocks());

  it('ignores a stale response that finishes after a newer analysis', async () => {
    const first = deferred<TemplateConfirmationRowDto[]>();
    const second = deferred<TemplateConfirmationRowDto[]>();
    vi.mocked(prepareTemplateSetup)
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => second.promise);
    const setStatus = vi.fn();
    const { result } = renderHook(() => useWorkspaceProfileInference(setStatus));

    const firstRefresh = result.current.refreshWorkspaceInference([pending('first')]);
    const secondRefresh = result.current.refreshWorkspaceInference([pending('second')]);

    await act(async () => {
      second.resolve(response(inference('Legal')));
      await secondRefresh;
    });
    await act(async () => {
      first.resolve(response(inference('Medical')));
      await firstRefresh;
    });

    expect(result.current.workspaceInference?.suggested_domain).toBe('Legal');
    expect(setStatus).not.toHaveBeenCalled();
  });
});
