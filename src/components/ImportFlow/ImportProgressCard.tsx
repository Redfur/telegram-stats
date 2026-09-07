import { Button } from "../ui";
import type { ImportProgress } from "../../hooks/useImportChat";
import { formatBytes } from "../../lib/formatBytes";
import styles from "./ImportFlow.module.css";

interface ImportProgressCardProps {
  progress: ImportProgress;
  onCancel: () => void;
}

// Макет «Экраны приложения» 3c, левая карточка. Процент — по байтам
// прочитанного файла, не по числу сообщений (см. docs/PLAN.md: второй
// проход файла только ради подсчёта противоречил бы цели «импорт 1M <
// 3 мин»), поэтому без ETA — оценка времени по неизвестному общему числу
// сообщений была бы честной только наполовину.
export function ImportProgressCard({ progress, onCancel }: ImportProgressCardProps) {
  const pct =
    progress.total_bytes != null && progress.total_bytes > 0
      ? Math.min(100, Math.round((progress.bytes_read / progress.total_bytes) * 100))
      : null;
  const bytesLabel = formatBytes(progress.bytes_read);
  const totalLabel = formatBytes(progress.total_bytes);

  return (
    <div className={styles.card}>
      <div className={["kick", styles.kicker].join(" ")}>ИМПОРТ</div>
      <h3 className={styles.title}>Разбор сообщений</h3>

      {pct != null ? (
        <>
          <div className={styles.pctRow}>
            <span className={styles.pctLabel}>{bytesLabel && totalLabel ? `${bytesLabel} из ${totalLabel}` : "Читаем файл"}</span>
            <span className={["num", styles.pct].join(" ")}>{pct}%</span>
          </div>
          <div className={styles.bar}>
            <div className={styles.barFill} style={{ width: `${pct}%` }} />
          </div>
        </>
      ) : (
        <div className={styles.bar}>
          <div className={[styles.barFill, styles.indeterminate].join(" ")} />
        </div>
      )}

      <div className={styles.counts}>
        <div>
          <div className={styles.countLabel}>СООБЩЕНИЙ</div>
          <div className={["num", styles.countValue].join(" ")}>
            {progress.message_count.toLocaleString("ru-RU")}
          </div>
        </div>
        <div>
          <div className={styles.countLabel}>ЛЮДЕЙ НАЙДЕНО</div>
          <div className={["num", styles.countValue].join(" ")}>
            {progress.people_count.toLocaleString("ru-RU")}
          </div>
        </div>
      </div>

      <div className={styles.actions}>
        <Button variant="secondary" onClick={onCancel}>
          Отменить
        </Button>
      </div>
    </div>
  );
}
