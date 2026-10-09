/** A failed Word scanner shutdown must never be represented as a completed cancellation. */
export interface GuidedScannerShutdownSnapshot {
  capture?: { document_closed?: boolean } | null;
  session: { session_id: string };
  target: { mode: string };
}

export async function closeGuidedScannerSafely(
  snapshot: GuidedScannerShutdownSnapshot,
  closeWord: (
    sessionId: string,
    discardWorkingCopy: boolean,
    onError: (detail: string) => void,
  ) => Promise<boolean | undefined>,
  setStatus: (message: string) => void,
): Promise<boolean> {
  if (snapshot.capture?.document_closed) return true;
  let detail: string | null = null;
  let closed = false;
  try {
    closed = (await closeWord(snapshot.session.session_id, snapshot.target.mode === 'template',
      (reason) => { detail = reason; })) === true;
  } catch (error) {
    detail = error instanceof Error ? error.message : String(error);
  }
  if (closed) return true;
  setStatus(`Не удалось закрыть сеанс Word-сканера${detail ? `: ${detail}` : '.'} Повторите закрытие; сеанс сохранён.`);
  return false;
}
