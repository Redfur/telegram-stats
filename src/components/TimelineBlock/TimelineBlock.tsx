import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Skeleton, UnavailableCard } from "../ui";
import { TimelineHeader } from "./TimelineHeader";
import { TimelineChart } from "./TimelineChart";
import type { PersonTimelineRow, TimelineOverview } from "../../lib/types";
import styles from "./TimelineBlock.module.css";

interface TimelineBlockProps {
  chatId: number;
  onReimport: () => void;
}

// Та же логика дебаунса, что и в Dashboard.tsx/ReactionsBlock.tsx — не
// бить invoke на каждый промежуточный chatId при быстром переключении.
const SWITCH_DEBOUNCE_MS = 150;

// Блок «Таймлайн участников» (макет «Дашборд», ход t10/10a — «Таймлайн
// участников — схлопнутый»): кто когда вошёл/вышел/был удалён, состав
// чата во времени, групповые события (создание, смена названия/фото,
// миграция). Сервисные события доступны только на момент импорта (как
// реакции) — self-heal невозможен, только переимпорт, отсюда тот же
// паттерн needs_reimport/UnavailableCard.
export function TimelineBlock({ chatId, onReimport }: TimelineBlockProps) {
  const [overview, setOverview] = useState<TimelineOverview | null>(null);
  const [hoveredPerson, setHoveredPerson] = useState<PersonTimelineRow | null>(null);

  useEffect(() => {
    let cancelled = false;
    setOverview(null);
    setHoveredPerson(null);
    const timer = setTimeout(() => {
      invoke<TimelineOverview>("get_participant_timeline", { chatId }).then((r) => {
        if (!cancelled) setOverview(r);
      });
    }, SWITCH_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [chatId]);

  if (overview == null) {
    return (
      <div className={styles.wrap} aria-busy="true">
        <Card>
          <Skeleton height={220} />
        </Card>
      </div>
    );
  }

  if (overview.status === "needs_reimport") {
    return (
      <UnavailableCard
        title="Таймлайн участников"
        message="Файл был импортирован до появления этой метрики — история входов и выходов ещё не сохранена. Переимпортируйте чат, чтобы увидеть таймлайн."
        actionLabel="Обновить выгрузку"
        onAction={onReimport}
      />
    );
  }

  if (overview.status === "not_applicable") {
    return (
      <UnavailableCard
        title="Таймлайн участников"
        message="В этом чате нет истории вступлений и выходов — обычно так для личной переписки или чата без единого зафиксированного события."
      />
    );
  }

  const summary = overview;

  return (
    <div className={styles.wrap}>
      {summary.unresolved_event_count > 0 && (
        <div className={styles.caveat}>
          {summary.unresolved_event_count === 1
            ? "1 событие не удалось сопоставить с конкретным участником — Telegram отдаёт в выгрузке только имя"
            : `${summary.unresolved_event_count} событий не удалось сопоставить с конкретным участником — Telegram отдаёт в выгрузке только имя`}
        </div>
      )}
      <Card>
        <TimelineHeader summary={summary} hoveredPerson={hoveredPerson} />
        <TimelineChart summary={summary} onHoverPerson={setHoveredPerson} />
      </Card>
    </div>
  );
}
