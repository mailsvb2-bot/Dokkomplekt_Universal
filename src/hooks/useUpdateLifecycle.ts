import { useEffect } from 'react';
import { applyVerifiedUpdate, checkForUpdates, getUpdateRecoveryStatus } from '../lib/api';
import { actionErrorMessage } from './useActionRunner';

type RunAction = <T>(
  label: string,
  action: () => Promise<T>,
  onError?: (detail: string) => void,
) => Promise<T | undefined>;

type ConfirmOptions = {
  title: string;
  message: string;
  confirmLabel?: string;
  danger?: boolean;
};

type UseUpdateLifecycleOptions = {
  workspaceStateReady: boolean;
  run: RunAction;
  setStatus(message: string): void;
  confirm(options: ConfirmOptions): Promise<boolean>;
};

export function useUpdateLifecycle({
  workspaceStateReady,
  run,
  setStatus,
  confirm,
}: UseUpdateLifecycleOptions) {
  useEffect(() => {
    if (!workspaceStateReady) return;
    let alive = true;
    void getUpdateRecoveryStatus()
      .then((recovery) => {
        if (!alive || !recovery) return;
        if (recovery.status === 'verified') {
          setStatus(`Обновление до версии ${recovery.target_version} установлено и локальное состояние проверено.`);
        } else if (recovery.status === 'recoverable_failure') {
          setStatus(`Обновление не завершено: ${recovery.last_error ?? 'установщик не подтвердил новую версию'}. Резервная копия сохранена: ${recovery.backup_dir}.`);
        } else if (recovery.status === 'prepared' || recovery.status === 'installer_started') {
          setStatus(`Обновление до версии ${recovery.target_version} требует завершения. Recovery marker сохранён.`);
        } else {
          setStatus(`Неизвестное состояние восстановления обновления «${recovery.status}». Автоматическое продолжение обновления не выполняется.`);
        }
      })
      .catch((error) => {
        if (!alive) return;
        const detail = actionErrorMessage(error);
        setStatus(`Не удалось проверить состояние восстановления обновления: ${detail}. Рабочий набор остаётся доступен; проверку обновления можно повторить вручную.`);
      });
    return () => {
      alive = false;
    };
  }, [workspaceStateReady, setStatus]);

  async function checkAndApplyUpdate() {
    const result = await run('check_for_updates', () => checkForUpdates());
    if (!result) return;
    if (!result.available) {
      setStatus(`${result.message}: ${result.current_version}.`);
      return;
    }
    if (!result.verified_package_path || !result.sha256 || !result.size_bytes) {
      setStatus(`Доступна версия ${result.latest_version}, но проверенный пакет неполон. Установка не запущена.`);
      return;
    }

    setStatus(`Доступна версия ${result.latest_version}. Подпись manifest, размер и SHA-256 пакета проверены.`);
    const confirmed = await confirm({
      title: `Установить обновление ${result.latest_version}?`,
      message: 'Программа сохранит резервную копию локального состояния, повторно проверит пакет, запустит подписанный NSIS и завершит работу. После установки откройте Доккомплект снова.',
      confirmLabel: 'Установить и перезапустить',
    });
    if (!confirmed) {
      setStatus(`Обновление ${result.latest_version} проверено и скачано, но установка отменена пользователем.`);
      return;
    }

    const verifiedPackagePath = result.verified_package_path;
    const verifiedSha256 = result.sha256;
    const verifiedSizeBytes = result.size_bytes;
    const targetVersion = result.latest_version;
    const applied = await run('apply_verified_update', () => applyVerifiedUpdate(
      verifiedPackagePath,
      targetVersion,
      verifiedSha256,
      verifiedSizeBytes,
    ));
    if (applied) setStatus(applied.message);
  }

  return { checkAndApplyUpdate };
}
