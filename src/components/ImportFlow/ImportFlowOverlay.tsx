import type { ImportErrorInfo, ImportProgress } from "../../hooks/useImportChat";
import { ImportProgressCard } from "./ImportProgressCard";
import { ImportErrorCard } from "./ImportErrorCard";
import styles from "./ImportFlow.module.css";

interface ImportFlowOverlayProps {
  progress: ImportProgress | null;
  error: ImportErrorInfo | null;
  onCancel: () => void;
  onPickAnother: () => void;
  onDismiss: () => void;
}

// Показывается только когда есть что показать: маленький файл может
// импортироваться быстрее первого события прогресса — тогда оверлей не
// появляется вовсе (SPEC §6), без искусственного таймера-заглушки.
export function ImportFlowOverlay({ progress, error, onCancel, onPickAnother, onDismiss }: ImportFlowOverlayProps) {
  if (error?.kind === "cancelled") return null; // пользователь сам отменил — не ошибка, тихо закрываем
  if (!progress && !error) return null;

  return (
    <div className={styles.overlay}>
      {error ? (
        <ImportErrorCard error={error} onPickAnother={onPickAnother} onDismiss={onDismiss} />
      ) : (
        progress && <ImportProgressCard progress={progress} onCancel={onCancel} />
      )}
    </div>
  );
}
