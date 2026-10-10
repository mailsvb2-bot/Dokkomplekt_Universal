import { useRef } from 'react';
import type {
  BundleDecision, DocumentRoutingRecommendation, GeneratedOutput,
  ParseSourceFileResponse, RecognitionProofEntry, SemanticExtractResult, WorkflowPlan,
} from '../lib/types';
import { parseSource, parseSourceFile, parseSourcePath, parseWebSource, pickSourceFile } from '../lib/api';
import { arrayBufferToBase64, currentDefaultYear, readFileBytes, semanticPreviewFromParsedSource } from '../lib/appSupport';

type ParsedSourceSummary = {
  title: string; count: number; warnings: string[];
  sourceKind?: string; layoutRows?: number; tableRows?: number;
};
type RunAction = <T>(name: string, action: () => Promise<T>, onError?: (detail: string) => void) => Promise<T | undefined>;

interface SourceIngressOptions {
  sourceText: string;
  sourceFileName: string | null;
  sourceFilePath: string | null;
  webSourceUrl: string;
  parsed: ParsedSourceSummary | null;
  semantic: SemanticExtractResult | null;
  preflightPlan: WorkflowPlan | null;
  lastOutput: GeneratedOutput | null;
  setSourceText(value: string): void;
  setSourceFileName(value: string | null): void;
  setSourceFilePath(value: string | null): void;
  setWebSourceUrl(value: string): void;
  setParsed(value: ParsedSourceSummary | null): void;
  setRecognitionProof(value: RecognitionProofEntry[]): void;
  setSemantic(value: SemanticExtractResult | null): void;
  setPlan(value: WorkflowPlan | null): void;
  setPreflightPlan(value: WorkflowPlan | null): void;
  setPreview(value: { text: string; missing: number; label: string } | null): void;
  setLastOutput(value: GeneratedOutput | null): void;
  setStatus(message: string): void;
  run: RunAction;
  clearSourceScopedUiState(): void;
  ensureComponentForSource(fileName: string): Promise<boolean>;
  applyBundleDecision(decision: BundleDecision, routing: DocumentRoutingRecommendation): string;
}

/** Owns the single accepted source revision across native file, drop, web and text ingress. */
export function useSourceIngress(options: SourceIngressOptions) {
  const {
    sourceText, sourceFileName, sourceFilePath, webSourceUrl, parsed, semantic, preflightPlan,
    lastOutput, setSourceText, setSourceFileName, setSourceFilePath, setWebSourceUrl,
    setParsed, setRecognitionProof, setSemantic, setPlan, setPreflightPlan, setPreview,
    setLastOutput, setStatus, run, clearSourceScopedUiState, ensureComponentForSource,
    applyBundleDecision,
  } = options;
  const sourceRevision = useRef(0);
  const acceptedSourceRevision = useRef<number | null>(null);

  function invalidateSource(): number {
    sourceRevision.current += 1;
    acceptedSourceRevision.current = null;
    return sourceRevision.current;
  }

  function acceptedRevision(): number | null {
    return acceptedSourceRevision.current === sourceRevision.current
      ? acceptedSourceRevision.current
      : null;
  }

  // A native/WebView source replacement is a transaction: the Rust intake
  // owns the old SemanticCase until the new input has been validated and the
  // canonical state transaction commits. Keep the last accepted UI snapshot,
  // but suspend publication until that commit (or a verified failure).
  function beginSourceReplacement(): { revision: number; previousAccepted: boolean } {
    const previousAccepted = acceptedRevision() !== null;
    const revision = invalidateSource();
    return { revision, previousAccepted };
  }

  function cancelSourceReplacement(attempt: { revision: number; previousAccepted: boolean }) {
    // A later edit/reset/replacement takes precedence. Never revive an old
    // source revision if a newer operation has already started.
    if (sourceRevision.current === attempt.revision && attempt.previousAccepted) {
      acceptedSourceRevision.current = attempt.revision;
    }
  }

  function changeSourceText(value: string) {
    invalidateSource();
    setSourceText(value);
    if (parsed || sourceFileName || sourceFilePath || semantic || preflightPlan || lastOutput) {
      setSourceFileName(null);
      setSourceFilePath(null);
      setParsed(null);
      setSemantic(null);
      setPlan(null);
      setPreflightPlan(null);
      setPreview(null);
      setLastOutput(null);
      setStatus('Текст источника изменён. Нажмите «Использовать текст», чтобы заново распознать данные перед созданием документов.');
    }
  }

  async function parseSourceNow() {
    // Unlike picking an alternate file, manual text was already explicitly
    // edited. Failure to parse it must not resurrect the prior source.
    const revision = invalidateSource();
    setParsed(null);
    clearSourceScopedUiState();
    const res = await run('parse_source', () => parseSource(sourceText, currentDefaultYear()));
    if (!res || revision !== sourceRevision.current) return;
    acceptedSourceRevision.current = revision;
    setSourceFileName(null);
    setSourceFilePath(null);
    setWebSourceUrl('');
    clearSourceScopedUiState();
    setSemantic(semanticPreviewFromParsedSource(res));
    const count = Object.keys(res.semantic_case?.values ?? {}).length;
    setParsed({
      title: res.report?.recognized_title ?? 'Документ распознан',
      count,
      warnings: res.report?.warnings ?? [],
      sourceKind: 'manual_text',
      layoutRows: 0,
      tableRows: 0,
    });
    const routingSummary = applyBundleDecision(res.bundle_decision, res.routing);
    setStatus(`Источник прочитан. Найдено значений: ${count}.${routingSummary}`);
  }

  async function applyParsedSourceFile(res: ParseSourceFileResponse, fileName: string, revision: number) {
    if (revision !== sourceRevision.current) return;
    acceptedSourceRevision.current = revision;
    clearSourceScopedUiState();
    setSourceFileName(fileName);
    setSourceFilePath(res.source_path);
    setSourceText(res.source_text);
    setWebSourceUrl('');
    setRecognitionProof(res.recognition_proof ?? []);
    setSemantic(semanticPreviewFromParsedSource(res));
    const count = Object.keys(res.semantic_case?.values ?? {}).length;
    const layoutItems = res.layout_items ?? [];
    setParsed({
      title: res.report?.recognized_title ?? fileName,
      count,
      warnings: res.report?.warnings ?? [],
      sourceKind: res.source_kind ?? 'file',
      layoutRows: layoutItems.length,
      tableRows: layoutItems.filter((item) => item.item_kind === 'table_row').length,
    });
    const routingSummary = applyBundleDecision(res.bundle_decision, res.routing);
    setStatus(`Файл «${fileName}» прочитан. Найдено значений: ${count}.${routingSummary}`);
  }

  async function pickSourceFileNative() {
    const picked = await run('pick_source_file', () => pickSourceFile());
    if (!picked) return;
    const attempt = beginSourceReplacement();
    try {
      if (!(await ensureComponentForSource(picked.file_name))) return;
      const res = await run('parse_source_path', () => parseSourcePath(picked.selected_path, currentDefaultYear()));
      if (!res) return;
      await applyParsedSourceFile(res, picked.file_name, attempt.revision);
    } finally {
      cancelSourceReplacement(attempt);
    }
  }

  async function processSourceFile(file: File) {
    const attempt = beginSourceReplacement();
    try {
      if (!(await ensureComponentForSource(file.name))) return;
      const buffer = await readFileBytes(file);
      const res = await run('parse_source_file', () =>
        parseSourceFile(file.name, arrayBufferToBase64(buffer), currentDefaultYear()));
      if (!res) return;
      await applyParsedSourceFile(res, file.name, attempt.revision);
    } finally {
      cancelSourceReplacement(attempt);
    }
  }


  async function loadWebSource() {
    const url = webSourceUrl.trim();
    if (!url) {
      setStatus('Укажите HTTPS-адрес сайта или API.');
      return;
    }
    const attempt = beginSourceReplacement();
    const res = await run('parse_web_source', () => parseWebSource(url, currentDefaultYear()));
    if (!res || attempt.revision !== sourceRevision.current) {
      cancelSourceReplacement(attempt);
      return;
    }
    acceptedSourceRevision.current = attempt.revision;
    clearSourceScopedUiState();
    setSourceFileName(res.final_url);
    setSourceFilePath(null);
    setSourceText(res.source_text);
    setSemantic(semanticPreviewFromParsedSource(res));
    const count = Object.keys(res.semantic_case?.values ?? {}).length;
    setParsed({
      title: res.report?.recognized_title ?? res.final_url,
      count,
      warnings: res.report?.warnings ?? [],
      sourceKind: 'https',
      layoutRows: 0,
      tableRows: 0,
    });
    const routingSummary = applyBundleDecision(res.bundle_decision, res.routing);
    setStatus(`Источник загружен. Найдено значений: ${count}.${routingSummary}`);
  }


  return {
    invalidateSource, acceptedRevision, changeSourceText, parseSourceNow,
    pickSourceFileNative, processSourceFile, loadWebSource,
  };
}
