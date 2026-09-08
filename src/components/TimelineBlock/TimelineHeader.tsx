import { formatDurationDays } from "../../lib/formatDuration";
import { formatShortDate } from "../../lib/formatMessageTimestamp";
import { seriesColorVar } from "./timelineColors";
import type { PersonTimelineRow, TimelineSummary } from "../../lib/types";
import styles from "./TimelineBlock.module.css";

interface TimelineHeaderProps {
  summary: TimelineSummary;
  hoveredPerson: PersonTimelineRow | null;
}

function personRangeLabel(person: PersonTimelineRow): string {
  const first = person.segments[0];
  const last = person.segments[person.segments.length - 1];
  if (!first) return "—";
  const start = formatShortDate(first.start_ts);
  if (last.end_ts == null) return `с ${start}`;
  return `${start} — ${formatShortDate(last.end_ts)}`;
}

function personTotalTenureDays(person: PersonTimelineRow, asOfTs: number | null): number {
  let total = 0;
  for (const seg of person.segments) {
    const end = seg.end_ts ?? asOfTs ?? seg.start_ts;
    total += Math.max(0, end - seg.start_ts) / 86_400;
  }
  return total;
}

// Правая часть шапки блока переключается по наведению на строку/линию
// участника в TimelineChart: по умолчанию — три сводных показателя
// (медианный срок/пик состава/события), при наведении — капсула с
// деталями конкретного человека. Сам факт наведения поднят в
// TimelineBlock, чтобы им управляли и шапка, и график synchронно.
export function TimelineHeader({ summary, hoveredPerson }: TimelineHeaderProps) {
  const rowIndex = hoveredPerson
    ? summary.people.findIndex((p) => p.person_id === hoveredPerson.person_id)
    : -1;

  return (
    <div className={styles.header}>
      <div className={styles.headerTotal}>
        <div className={styles.headerLabel}>Прошло через чат</div>
        <div className={styles.headerTotalRow}>
          <span className={styles.headerTotalValue}>{summary.total.toLocaleString("ru-RU")}</span>
          <span className={styles.headerTotalCaption}>
            сейчас {summary.current.toLocaleString("ru-RU")} · вышли{" "}
            {summary.left.toLocaleString("ru-RU")}
          </span>
        </div>
      </div>

      <div className={styles.headerRight}>
        {hoveredPerson ? (
          <div className={styles.capsule}>
            <div className={styles.capsuleName}>
              <span
                className={styles.capsuleSwatch}
                style={{ background: seriesColorVar(rowIndex) }}
              />
              {hoveredPerson.display_name || "Без имени"}
            </div>
            <div className={styles.capsuleMeta}>
              <span>{personRangeLabel(hoveredPerson)}</span>
              <span>
                в группе {formatDurationDays(personTotalTenureDays(hoveredPerson, summary.as_of_ts))}
              </span>
            </div>
          </div>
        ) : (
          <div className={styles.headerStats}>
            <div className={styles.headerStat}>
              <div className={styles.headerStatLabel}>Медианный срок</div>
              <div className={styles.headerStatValue}>
                {summary.median_tenure_days != null
                  ? formatDurationDays(summary.median_tenure_days)
                  : "—"}
              </div>
            </div>
            <div className={styles.headerStat}>
              <div className={styles.headerStatLabel}>Максимум состава</div>
              <div className={styles.headerStatValue}>
                {summary.peak_headcount.toLocaleString("ru-RU")}
              </div>
            </div>
            <div className={styles.headerStat}>
              <div className={styles.headerStatLabel}>События</div>
              <div className={styles.headerStatValue}>
                {summary.group_event_count.toLocaleString("ru-RU")}
                {summary.last_group_event_ts != null && (
                  <div className={styles.headerStatSub}>
                    последнее {formatShortDate(summary.last_group_event_ts)}
                  </div>
                )}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
