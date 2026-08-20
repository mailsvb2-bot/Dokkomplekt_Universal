import { useCallback, useEffect, useState } from 'react';
import { prepareTemplateSetup } from '../lib/api';
import { errorMessage, type PendingTemplate } from '../lib/appSupport';
import type { DomainKind, TemplateCandidateDto, WorkspaceProfileInference, WorkspaceWorkflowShape } from '../lib/types';

export function pendingTemplateCandidates(items: PendingTemplate[]): TemplateCandidateDto[] {
  return items.map((item) => ({
    document_id: item.document_id,
    template_path: item.template_path,
    extracted_text: item.extracted_text,
    preferred_button_label: item.button_label.trim() || item.file_name.replace(/\.doc[xm]$/i, ''),
    domain_override: item.domain_override,
  }));
}

export function applyWorkspaceDomainToPending(items: PendingTemplate[], domain: DomainKind): PendingTemplate[] {
  return items.map((item) => ({ ...item, domain_override: domain }));
}

export function useWorkspaceProfileInference(setStatus: (value: string) => void, items: PendingTemplate[] = []) {
  const [workspaceInference, setWorkspaceInference] = useState<WorkspaceProfileInference | null>(null);
  const [workspaceShape, setWorkspaceShape] = useState<WorkspaceWorkflowShape | null>(null);

  const refreshWorkspaceInference = useCallback(async (templates: PendingTemplate[]) => {
    if (!templates.length) {
      setWorkspaceInference(null);
      setWorkspaceShape(null);
      return null;
    }
    try {
      const rows = await prepareTemplateSetup(pendingTemplateCandidates(templates));
      const inference = rows[0]?.workspace_inference ?? null;
      setWorkspaceInference(inference);
      setWorkspaceShape(rows[0]?.workspace_shape ?? null);
      return inference;
    } catch (error) {
      setWorkspaceInference(null);
      setWorkspaceShape(null);
      setStatus(`Не удалось определить рабочий профиль: ${errorMessage(error)}. Кнопки можно создать без выбора профессии.`);
      return null;
    }
  }, [setStatus]);

  useEffect(() => {
    if (!items.length) { setWorkspaceInference(null); setWorkspaceShape(null); return; }
    const timer = window.setTimeout(() => { void refreshWorkspaceInference(items); }, 250);
    return () => window.clearTimeout(timer);
  }, [items, refreshWorkspaceInference]);

  return { workspaceInference, workspaceShape, setWorkspaceInference, setWorkspaceShape, refreshWorkspaceInference };
}
