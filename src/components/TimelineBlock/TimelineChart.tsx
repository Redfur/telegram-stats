import { useState } from "react";
import { formatFullDateTime } from "../../lib/formatMessageTimestamp";
import { seriesColorVar } from "./timelineColors";
import type { PersonTimelineRow, TimelineGroupEventKind, TimelineSummary } from "../../lib/types";
import styles from "./TimelineBlock.module.css";

interface TimelineChartProps {
  summary: TimelineSummary;
  onHoverPerson: (person: PersonTimelineRow | null) => void;
}

const GROUP_EVENT_LABELS: Record<TimelineGroupEventKind, string> = {
  created: "Создание чата",
  title_changed: "Смена названия",
  photo_changed: "Смена фото",
  migrated: "Миграция в супергруппу",
};

const PERSON_EVENT_LABELS: Record<string, string> = {
  joined: "вход",
  left: "выход",
  removed: "удаление",
};

interface DotTooltip {
  leftPct: number;
  title: string;
  date: string;
}

// Общий диапазон времени для всех слоёв графика (композиция, лента
// групповых событий, строки участников) — не только по составу: точки
// событий/сегментов могут лежать за пределами диапазона композиции в
// вырожденных случаях (например единственное групповое событие без
// членских).
function computeDomain(summary: TimelineSummary): [number, number] {
  let min = Infinity;
  let max = -Infinity;
  const consider = (ts: number) => {
    if (ts < min) min = ts;
    if (ts > max) max = ts;
  };
  for (const p of summary.composition) consider(p.ts);
  for (const e of summary.group_events) consider(e.ts);
  for (const person of summary.people) {
    for (const seg of person.segments) {
      consider(seg.start_ts);
      if (seg.end_ts != null) consider(seg.end_ts);
    }
  }
  if (!Number.isFinite(min) || !Number.isFinite(max) || max <= min) {
    return [min, min + 1];
  }
  return [min, max];
}

function pctOf(ts: number, min: number, max: number): number {
  if (max <= min) return 50;
  return ((ts - min) / (max - min)) * 100;
}

function yearTicks(min: number, max: number): { ts: number; year: number }[] {
  const startYear = new Date(min * 1000).getUTCFullYear();
  const endYear = new Date(max * 1000).getUTCFullYear();
  const ticks: { ts: number; year: number }[] = [];
  for (let y = startYear; y <= endYear; y++) {
    const ts = Date.UTC(y, 0, 1) / 1000;
    if (ts >= min && ts <= max) ticks.push({ ts, year: y });
  }
  return ticks;
}

// Ступенчатая линия, не диагональная интерполяция между точками — состав
// чата меняется скачками в момент входа/выхода, не плавно между ними.
function compositionStepPoints(summary: TimelineSummary, min: number, max: number): string {
  const pts = summary.composition;
  if (pts.length === 0) return "";
  const peak = Math.max(1, ...pts.map((p) => p.count));
  const y = (count: number) => 100 - (count / peak) * 100;
  const x = (ts: number) => pctOf(ts, min, max);

  const out: string[] = [`${x(min)},${y(0)}`, `${x(pts[0].ts)},${y(0)}`];
  for (let i = 0; i < pts.length; i++) {
    out.push(`${x(pts[i].ts)},${y(pts[i].count)}`);
    const nextTs = i + 1 < pts.length ? pts[i + 1].ts : max;
    out.push(`${x(nextTs)},${y(pts[i].count)}`);
  }
  return out.join(" ");
}

// Тело блока — композиция сверху, лента групповых событий, строки
// участников (сегменты — сплошные, промежутки между выходом и повторным
// входом — пунктир), годовые метки снизу. Позиционирование — проценты от
// общего временного диапазона, тем же приёмом, что и диаграмма-весы в
// ReactionPersonFilter (div вместо recharts — нестандартная форма чарта).
// Точки событий рисуются отдельно от границ сегментов (`person.events`,
// не `person.segments`) — у объединённых слиянием аккаунтов событие
// может лежать внутри уже непрерывного покрытия, не только на его концах.
// Тултип — свой (не нативный `title`): в WebView нативные тултипы на
// маленьких абсолютно позиционированных точках срабатывали ненадёжно.
export function TimelineChart({ summary, onHoverPerson }: TimelineChartProps) {
  const [tooltip, setTooltip] = useState<DotTooltip | null>(null);
  const [min, max] = computeDomain(summary);
  const x = (ts: number) => pctOf(ts, min, max);
  const ticks = yearTicks(min, max);
  const peakCount = Math.max(1, ...summary.composition.map((p) => p.count));

  return (
    <div className={styles.chart}>
      <div className={styles.compositionHead}>
        <span className={styles.compositionLabel}>Состав</span>
        <span className={styles.compositionPeak}>пик {peakCount.toLocaleString("ru-RU")}</span>
        <span className={styles.groupEventsLabel}>события группы ↓</span>
      </div>
      <svg viewBox="0 0 100 100" preserveAspectRatio="none" className={styles.compositionSvg}>
        <polyline
          points={compositionStepPoints(summary, min, max)}
          className={styles.compositionLine}
        />
      </svg>

      <div className={styles.plot} onMouseLeave={() => setTooltip(null)}>
        <div className={styles.groupEventsRow}>
          <div className={styles.groupEventsAxis} />
          {summary.group_events.map((e, i) => (
            <span
              key={i}
              className={styles.groupDot}
              style={{ left: `${x(e.ts)}%` }}
              onMouseEnter={() =>
                setTooltip({
                  leftPct: x(e.ts),
                  title:
                    GROUP_EVENT_LABELS[e.kind] + (e.detail ? ` → «${e.detail}»` : ""),
                  date: formatFullDateTime(e.ts),
                })
              }
            />
          ))}
        </div>

        <div className={styles.rows}>
          {summary.people.map((person, rowIndex) => {
            const color = seriesColorVar(rowIndex);
            const name = person.display_name || "Без имени";
            return (
              <div
                key={person.person_id}
                className={styles.row}
                onMouseEnter={() => onHoverPerson(person)}
                onMouseLeave={() => onHoverPerson(null)}
              >
                {person.segments.map((seg, si) => {
                  const prevEnd = si > 0 ? person.segments[si - 1].end_ts : null;
                  const segStartPct = x(seg.start_ts);
                  const segEndPct = x(seg.end_ts ?? max);
                  return (
                    <div key={si}>
                      {prevEnd != null && (
                        <div
                          className={styles.gap}
                          style={{
                            left: `${x(prevEnd)}%`,
                            width: `${Math.max(0, segStartPct - x(prevEnd))}%`,
                          }}
                        />
                      )}
                      <div
                        className={styles.segment}
                        style={{
                          left: `${segStartPct}%`,
                          width: `${Math.max(0, segEndPct - segStartPct)}%`,
                          background: color,
                        }}
                      />
                    </div>
                  );
                })}
                {person.events.map((e, ei) => (
                  <span
                    key={ei}
                    className={styles.personDot}
                    style={{
                      left: `${x(e.ts)}%`,
                      borderColor: color,
                      background: e.kind === "joined" ? color : "var(--surface)",
                    }}
                    onMouseEnter={() =>
                      setTooltip({
                        leftPct: x(e.ts),
                        title: `${name} — ${PERSON_EVENT_LABELS[e.kind] ?? e.kind}`,
                        date: formatFullDateTime(e.ts),
                      })
                    }
                  />
                ))}
              </div>
            );
          })}
        </div>

        {tooltip && (
          <div className={styles.tooltip} style={{ left: `${tooltip.leftPct}%` }}>
            <div className={styles.tooltipTitle}>{tooltip.title}</div>
            <div className={styles.tooltipDate}>{tooltip.date}</div>
          </div>
        )}
      </div>

      <div className={styles.yearAxis}>
        {ticks.map((t) => (
          <span key={t.year} className={styles.yearTick} style={{ left: `${x(t.ts)}%` }}>
            '{String(t.year).slice(-2)}
          </span>
        ))}
      </div>
    </div>
  );
}
