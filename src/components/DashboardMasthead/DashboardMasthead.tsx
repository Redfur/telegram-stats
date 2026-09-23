import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Tag } from "../ui";
import { formatPeriod } from "../../lib/formatPeriod";
import { formatChatType } from "../../lib/formatChatType";
import { formatShortDate } from "../../lib/formatMessageTimestamp";
import { formatDurationDays } from "../../lib/formatDuration";
import type { ChatTotals, TitleHistory } from "../../lib/types";
import styles from "./DashboardMasthead.module.css";

interface DashboardMastheadProps {
  chatId: number;
  chatName: string | null;
  chatType: string;
  totals: ChatTotals | null;
}

// Тот же debounce, что у остальных per-chat блоков дашборда
// (Dashboard.tsx) — быстрый скролл по рельсу не должен бить invoke на
// каждый промежуточный chatId.
const SWITCH_DEBOUNCE_MS = 150;

// Тултип наведения на сегмент скрывается с небольшой задержкой — тот же
// приём, что в TimelineChart.tsx (HIDE_DELAY_MS): курсору нужно время
// долететь до самого тултипа.
const HIDE_DELAY_MS = 100;

interface SegmentTooltip {
  index: number;
  leftPct: number;
}

function tooltipTransform(leftPct: number): string {
  if (leftPct < 12) return "translate(0, 0)";
  if (leftPct > 88) return "translate(-100%, 0)";
  return "translate(-50%, 0)";
}

// Шапка страницы дашборда (макет «Дашборд», ход t11/вариант 11a) —
// тип чата, период и название; сообщения/участники сознательно не
// дублируются здесь — та же сводка секцией ниже (00 СВОДКА). Если чат
// хоть раз переименовывался, под названием — полоса истории названий:
// ширина сегмента пропорциональна сроку действия названия (flexGrow
// вместо ручного расчёта % через left/width — сумма долей сама даёт
// 100% без отдельной арифметики), тултип на hover — имя, период, срок,
// кто переименовал. Полоса просто не рендерится, если название ни разу
// не менялось или история недоступна (чат импортирован до миграции
// таймлайна) — вырожденная метрика скрывается целиком, без поясняющей
// карточки (docs/SPEC.md §7), backend не различает эти два случая
// специально. Первый сегмент без `created`-события в источнике честно
// помечен «название неизвестно» — тултип поясняет причину отдельной
// строкой, не просто заглушка (реальный случай: экспорт не содержит
// истории с основания чата).
export function DashboardMasthead({ chatId, chatName, chatType, totals }: DashboardMastheadProps) {
  const [history, setHistory] = useState<TitleHistory | null>(null);
  const [tooltip, setTooltip] = useState<SegmentTooltip | null>(null);
  const hideTimer = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    setHistory(null);
    setTooltip(null);
    const timer = setTimeout(() => {
      invoke<TitleHistory | null>("get_title_history", { chatId }).then((r) => {
        if (!cancelled) setHistory(r);
      });
    }, SWITCH_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [chatId]);

  const cancelHide = () => {
    if (hideTimer.current != null) {
      window.clearTimeout(hideTimer.current);
      hideTimer.current = null;
    }
  };
  const scheduleHide = () => {
    cancelHide();
    hideTimer.current = window.setTimeout(() => {
      setTooltip(null);
      hideTimer.current = null;
    }, HIDE_DELAY_MS);
  };

  const period = totals && formatPeriod(totals.first_msg_ts, totals.last_msg_ts);

  const segments = history?.segments ?? [];
  const overallStart = segments[0]?.start_ts;
  const overallEnd =
    segments.length > 0
      ? (segments[segments.length - 1].end_ts ?? totals?.last_msg_ts ?? segments[segments.length - 1].start_ts)
      : undefined;

  return (
    <header className={styles.masthead}>
      <div className={styles.head}>
        <Tag variant="outline">{formatChatType(chatType)}</Tag>
        {period && <span className={styles.period}>{period}</span>}
      </div>
      <h1 className={styles.title}>{chatName || "Без названия"}</h1>

      {segments.length > 0 && overallStart != null && overallEnd != null && (
        <div className={styles.segments} onMouseLeave={scheduleHide}>
          {segments.map((seg, i) => {
            const segEnd = seg.end_ts ?? overallEnd;
            const duration = Math.max(1, segEnd - seg.start_ts);
            const leftPct = ((seg.start_ts - overallStart) / Math.max(1, overallEnd - overallStart)) * 100;
            return (
              <div
                key={i}
                className={styles.segment}
                style={{ flexGrow: duration }}
                onMouseEnter={() => {
                  cancelHide();
                  setTooltip({ index: i, leftPct });
                }}
              >
                <span className={styles.segmentLabel}>{seg.name ?? "название неизвестно"}</span>
              </div>
            );
          })}
          {tooltip &&
            (() => {
              const seg = segments[tooltip.index];
              const segEnd = seg.end_ts ?? overallEnd;
              const days = (segEnd - seg.start_ts) / 86400;
              return (
                <div
                  className={styles.tooltip}
                  style={{ left: `${tooltip.leftPct}%`, transform: tooltipTransform(tooltip.leftPct) }}
                  onMouseEnter={cancelHide}
                  onMouseLeave={() => setTooltip(null)}
                >
                  <div className={styles.tooltipName}>«{seg.name ?? "название неизвестно"}»</div>
                  <div className={styles.tooltipMeta}>
                    {formatShortDate(seg.start_ts)} — {seg.end_ts != null ? formatShortDate(seg.end_ts) : "сейчас"}{" "}
                    · {formatDurationDays(days)}
                  </div>
                  {seg.author_display_name && (
                    <div className={styles.tooltipMeta}>переименовал(а) {seg.author_display_name}</div>
                  )}
                  {seg.name == null && (
                    <div className={styles.tooltipMeta}>экспорт не содержит истории до этого момента</div>
                  )}
                </div>
              );
            })()}
        </div>
      )}
    </header>
  );
}
