import { CardKicker } from "../ui";
import type { MessageTypeKind, MessageTypeSummaryStats } from "../../lib/types";
import { MESSAGE_TYPE_LABELS } from "../../lib/messageTypeLabels";
import { formatDuration } from "../../lib/formatDuration";
import { formatTypeBucketLabel } from "../../lib/formatTypeBucketLabel";
import styles from "./MessageTypeSummary.module.css";

interface MessageTypeSummaryProps {
  kind: MessageTypeKind;
  totalCount: number;
  pctOfAll: number;
  summary: MessageTypeSummaryStats;
}

// Две ветки раскладки (макет 6a/6b): голосовые/кружочки несут длительность
// — суммарно/медиана/самое длинное, остальные десять типов — частоту:
// темп/пик/лидер.
export function MessageTypeSummary({ kind, totalCount, pctOfAll, summary }: MessageTypeSummaryProps) {
  const labels = MESSAGE_TYPE_LABELS[kind];
  const pct = pctOfAll.toFixed(1).replace(".", ",");

  return (
    <div className={styles.head}>
      <div className={styles.main}>
        <CardKicker>Всего {labels.genitive}</CardKicker>
        <div className={styles.number}>{totalCount.toLocaleString("ru-RU")}</div>
        <div className={styles.pct}>{pct}% всех сообщений</div>
      </div>
      <div className={styles.grid}>
        {summary.kind === "duration" ? (
          <>
            <Cell label="Суммарно" value={formatDuration(summary.total_seconds)} />
            <Cell label="Медиана" value={formatDuration(summary.median_seconds)} />
            <Cell label="Самое длинное" value={formatDuration(summary.max_seconds)} />
          </>
        ) : (
          <>
            <Cell
              label={summary.rate_granularity === "month" ? "В месяц" : "В день"}
              value={Math.round(summary.rate_value).toLocaleString("ru-RU")}
            />
            <Cell
              label="Пик"
              value={
                summary.peak
                  ? formatTypeBucketLabel(summary.peak.bucket, summary.rate_granularity)
                  : "— мало данных"
              }
              muted={!summary.peak}
            />
            <Cell label="Чаще всех" value={summary.top_sender ?? "—"} />
          </>
        )}
      </div>
    </div>
  );
}

function Cell({ label, value, muted }: { label: string; value: string; muted?: boolean }) {
  return (
    <div className={styles.cell}>
      <div className={styles.cellLabel}>{label}</div>
      <div className={[styles.cellValue, muted ? styles.muted : ""].filter(Boolean).join(" ")}>{value}</div>
    </div>
  );
}
