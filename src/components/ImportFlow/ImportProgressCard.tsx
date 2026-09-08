import { useEffect, useState } from "react";
import { Button } from "../ui";
import type { ImportProgress } from "../../hooks/useImportChat";
import { formatBytes } from "../../lib/formatBytes";
import styles from "./ImportFlow.module.css";

interface ImportProgressCardProps {
  progress: ImportProgress;
  onCancel: () => void;
}

// Файл дочитан (phase: "finalizing"), но перед тем как чат появится в
// Библиотеке, ещё считаются агрегации (участники, метрики дашборда) —
// без измеримого процента. Вместо зависшего "100%" крутим подписи, без
// технических деталей — тот же принцип, что и у статус-слов в самом
// Claude Code во время работы.
const FINALIZING_CAPTIONS = [
  "Считаем всё, что можно посчитать…",
  "Раскладываем сообщения по полочкам…",
  "Сверяем, кто сколько писал…",
  "Наводим порядок в цифрах…",
  "Досчитываем последние проценты…",
  "Взвешиваем каждое сообщение…",
  "Раскидываем участников по местам…",
  "Уточняем детали…",
  "Шлифуем цифры…",
  "Ищем самых активных…",
  "Складываем статистику по кусочкам…",
  "Сверяем даты и часовые пояса…",
  "Приводим дашборд в боевую готовность…",
  "Почти готово…",
];

const CAPTION_INTERVAL_MS = 1800;

function useRotatingCaption(active: boolean): string {
  const [index, setIndex] = useState(() => Math.floor(Math.random() * FINALIZING_CAPTIONS.length));

  useEffect(() => {
    if (!active) return;
    const timer = setInterval(() => {
      setIndex((i) => (i + 1) % FINALIZING_CAPTIONS.length);
    }, CAPTION_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [active]);

  return FINALIZING_CAPTIONS[index];
}

// Макет «Экраны приложения» 3c, левая карточка. Процент — по байтам
// прочитанного файла, не по числу сообщений (см. docs/PLAN.md: второй
// проход файла только ради подсчёта противоречил бы цели «импорт 1M <
// 3 мин»), поэтому без ETA — оценка времени по неизвестному общему числу
// сообщений была бы честной только наполовину.
export function ImportProgressCard({ progress, onCancel }: ImportProgressCardProps) {
  const finalizing = progress.phase === "finalizing";
  const caption = useRotatingCaption(finalizing);

  const pct =
    !finalizing && progress.total_bytes != null && progress.total_bytes > 0
      ? Math.min(100, Math.round((progress.bytes_read / progress.total_bytes) * 100))
      : null;
  const bytesLabel = formatBytes(progress.bytes_read);
  const totalLabel = formatBytes(progress.total_bytes);

  return (
    <div className={styles.card}>
      <div className={["kick", styles.kicker].join(" ")}>ИМПОРТ</div>
      <h3 className={styles.title}>{finalizing ? "Считаем метрики" : "Разбор сообщений"}</h3>

      {finalizing ? (
        <>
          <div className={styles.pctRow}>
            <span className={styles.pctLabel}>{caption}</span>
          </div>
          <div className={styles.bar}>
            <div className={[styles.barFill, styles.indeterminate].join(" ")} />
          </div>
        </>
      ) : pct != null ? (
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
        <Button variant="secondary" onClick={onCancel} disabled={finalizing}>
          Отменить
        </Button>
      </div>
    </div>
  );
}
