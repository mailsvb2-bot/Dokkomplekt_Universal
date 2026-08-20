import { useCallback, useState } from 'react';
import { prepareTemplateSetup } from '../lib/api';
import { errorMessage, type PendingTemplate } from '../lib/appSupport';
import type { DomainKind, TemplateCandidateDto, WorkspaceProfileInference } from '../lib/types';

export function pendingTemplateCandidates(items: PendingTemplate[]): TemplateCandidateDto[] {
  return items.map((item) => ({
    document_id: item.document_id,
    template_path: item.template_path,
    extracted_text: item.extracted_text,
    preferred_button_label: item.button_label.trim() || item.file_name.replace(/\.doc[xm]$/i, ''),
  }));
}

export function applyWorkspaceDomainToPending(items: PendingTemplate[], domain: DomainKind): PendingTemplate[] {
  return items.map((item) => ({ ...item, domain_override: domain }));
}

export function useWorkspaceProfileInference(setStatus: (value: string) => void) {
  const [workspaceInference, setWorkspaceInference] = useState<WorkspaceProfileInference | null>(null);

  const refreshWorkspaceInference = useCallback(async (items: PendingTemplate[]) => {
    if (!items.length) {
      setWorkspaceInference(null);
      return null;
    }
    try {
      const rows = await prepareTemplateSetup(pendingTemplateCandidates(items));
      const inference = rows[0]?.workspace_inference ?? null;
      setWorkspaceInference(inference);
      return inference;
    } catch (error) {
      setWorkspaceInference(null);
      setStatus(`Не удалось определить рабочий профиль: ${errorMessage(error)}. Кнопки можно создать без выбора профессии.`);
      return null;
    }
  }, [setStatus]);

  return { workspaceInference, setWorkspaceInference, refreshWorkspaceInference };
}
