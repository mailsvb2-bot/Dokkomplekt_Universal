import { useEffect, useMemo, useState } from 'react';
import {
  deleteLearnedScannerRule,
  getLearnedKitDecision,
  listLearnedScannerRules,
  listTemplateApprovals,
  revokeDocumentTemplateApproval,
  type KitLearningDecision,
} from '../lib/api';
import { useAppDialog } from './AppDialogProvider';
import type { DocumentTemplateSpec, DomainKind, LearnedScannerRule, TemplateApprovalRecord } from '../lib/types';

interface LearningGovernancePanelProps {
  documents: DocumentTemplateSpec[];
  onStatus(message: string): void;
}

type BuiltinDomainKind = Exclude<DomainKind, { Custom: string }>;
type DomainChoice = BuiltinDomainKind | 'Custom';

const DOMAINS: Array<{ value: DomainChoice; label: string }> = [
  { value: 'Generic', label: 'Универсальный' },
  { value: 'Medical', label: 'Медицина' },
  { value: 'Legal', label: 'Право' },
  { value: 'Hr', label: 'Кадры' },
  { value: 'Accounting', label: 'Бухгалтерия' },
  { value: 'Education', label: 'Образование' },
  { value: 'Custom', label: 'Своя профессия' },
];

export function LearningGovernancePanel(props: LearningGovernancePanelProps) {
  const dialogs = useAppDialog();
  const [rules, setRules] = useState<LearnedScannerRule[]>([]);
  const [approvals, setApprovals] = useState<TemplateApprovalRecord[]>([]);
  const [domainChoice, setDomainChoice] = useState<DomainChoice>('Generic');
  const [customDomainId, setCustomDomainId] = useState('');
  const [clusterId, setClusterId] = useState('');
  const [packId, setPackId] = useState('');
  const [decision, setDecision] = useState<KitLearningDecision | null>(null);
  const [busy, setBusy] = useState(false);
  const documentLabels = useMemo(
    () => new Map(props.documents.map((document) => [document.id, document.button_label])),
    [props.documents],
  );

  useEffect(() => {
    let cancelled = false;
    void Promise.all([listLearnedScannerRules(), listTemplateApprovals()])
      .then(([loadedRules, loadedApprovals]) => {
        if (!cancelled) {
          setRules(loadedRules);
          setApprovals(loadedApprovals);
        }
      })
      .catch(() => { /* browser preview or unavailable Tauri bridge */ });
    return () => { cancelled = true; };
  }, []);

  async function refresh() {
    setBusy(true);
    try {
      const [loadedRules, loadedApprovals] = await Promise.all([
        listLearnedScannerRules(),
        listTemplateApprovals(),
      ]);
      setRules(loadedRules);
      setApprovals(loadedApprovals);
      props.onStatus(`Обученные правила: ${loadedRules.length}; подтверждённые шаблоны: ${loadedApprovals.length}.`);
    } catch (error) {
      props.onStatus(`Не удалось обновить управление обучением: ${messageOf(error)}`);
    } finally {
      setBusy(false);
    }
  }

  async function removeRule(rule: LearnedScannerRule) {
    const confirmed = await dialogs.confirm({ title: 'Удалить обученное правило?', message: `Правило «${rule.title || rule.field_id}» перестанет применяться к новым документам.`, confirmLabel: 'Удалить правило', danger: true });
    if (!confirmed) return;
    setBusy(true);
    try {
      setRules(await deleteLearnedScannerRule(rule.rule_id));
      props.onStatus(`Обученное правило «${rule.title || rule.field_id}» удалено.`);
    } catch (error) {
      props.onStatus(`Не удалось удалить правило: ${messageOf(error)}`);
    } finally {
      setBusy(false);
    }
  }

  async function revokeApproval(record: TemplateApprovalRecord) {
    const label = documentLabels.get(record.document_id) ?? record.document_id;
    const confirmed = await dialogs.confirm({ title: 'Отозвать подтверждение?', message: `Шаблон «${label}» снова потребует проверки перед использованием.`, confirmLabel: 'Отозвать подтверждение', danger: true });
    if (!confirmed) return;
    setBusy(true);
    try {
      setApprovals(await revokeDocumentTemplateApproval(record.document_id));
      props.onStatus(`Подтверждение шаблона «${label}» отозвано.`);
    } catch (error) {
      props.onStatus(`Не удалось отозвать подтверждение: ${messageOf(error)}`);
    } finally {
      setBusy(false);
    }
  }

  async function inspectDecision() {
    setDecision(null);
    const domain: DomainKind | null = domainChoice === 'Custom'
      ? (customDomainId.trim() ? { Custom: customDomainId.trim() } : null)
      : domainChoice;
    if (!domain) {
      props.onStatus('Укажите идентификатор своей профессии / профиля.');
      return;
    }
    if (!clusterId.trim()) {
      props.onStatus('Укажите идентификатор кластера для просмотра решения обученного комплекта.');
      return;
    }
    setBusy(true);
    try {
      const loaded = await getLearnedKitDecision(domain as unknown as string, clusterId.trim(), packId.trim() || undefined);
      setDecision(loaded);
      props.onStatus(loaded ? 'Решение обученного комплекта загружено.' : 'Для этого кластера ещё нет устойчивого обученного решения.');
    } catch (error) {
      props.onStatus(`Не удалось получить решение: ${messageOf(error)}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <details className="utilityCard governanceCard">
      <summary><strong>Обучение и подтверждения</strong></summary>
      <p>Здесь можно удалить ошибочно обученные правила, проверить решение автоподбора комплекта и отозвать подтверждение устаревшей версии шаблона.</p>
      <button className="softBtn" onClick={() => void refresh()} disabled={busy}>Обновить списки</button>

      <section aria-label="Обученные правила сканера">
        <h4>Правила распознавания</h4>
        {rules.length ? <ul className="neutralDataList">
          {rules.map((rule) => <li key={rule.rule_id}>
            <div>
              <strong>{rule.title || rule.field_id}</strong>
              <small>{rule.label_hint || 'Без подписи'} · {rule.learning_status ?? 'shadow'} · успешных применений: {rule.successful_applications ?? 0}</small>
            </div>
            <button className="textBtn" onClick={() => void removeRule(rule)} disabled={busy}>Удалить правило</button>
          </li>)}
        </ul> : <p>Сохранённых правил нет.</p>}
      </section>

      <section aria-label="Решение обученного комплекта">
        <h4>Автоподбор комплекта</h4>
        <div className="inlineInput governanceInputs">
          <select value={domainChoice} onChange={(event) => setDomainChoice(event.target.value as DomainChoice)} aria-label="Профиль решения">
            {DOMAINS.map((item) => <option key={item.value} value={item.value}>{item.label}</option>)}
          </select>
          {domainChoice === 'Custom' && <input
            value={customDomainId}
            onChange={(event) => setCustomDomainId(event.target.value)}
            placeholder="Своя профессия / профиль"
            aria-label="Своя профессия / профиль"
          />}
          <input value={clusterId} onChange={(event) => setClusterId(event.target.value)} placeholder="Идентификатор кластера" aria-label="Идентификатор кластера" />
          <input value={packId} onChange={(event) => setPackId(event.target.value)} placeholder="Набор шаблонов (необязательно)" aria-label="Идентификатор набора шаблонов" />
          <button className="softBtn" onClick={() => void inspectDecision()} disabled={busy}>Показать решение</button>
        </div>
        {decision && <div className="readyMessage" role="status">
          <i className="ti ti-brain" aria-hidden="true" />
          <div>
            <strong>{decision.auto_apply ? 'Разрешено автоприменение' : 'Требуется подтверждение специалиста'}</strong>
            <span>Уверенность: {(decision.confidence * 100).toFixed(0)}% · источник: {decision.source} · документов: {decision.document_ids.length}</span>
            <small>{decision.reason}</small>
          </div>
        </div>}
      </section>

      <section aria-label="Подтверждения шаблонов">
        <h4>Подтверждённые версии</h4>
        {approvals.length ? <ul className="neutralDataList">
          {approvals.map((record) => <li key={`${record.document_id}:${record.template_sha256}`}>
            <div>
              <strong>{documentLabels.get(record.document_id) ?? record.document_id}</strong>
              <small>{record.jurisdiction} · {record.approved_by} · {record.approved_at}</small>
            </div>
            <button className="textBtn" onClick={() => void revokeApproval(record)} disabled={busy}>Отозвать подтверждение</button>
          </li>)}
        </ul> : <p>Подтверждённых версий нет.</p>}
      </section>
    </details>
  );
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
