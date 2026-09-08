import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Skeleton, UnavailableCard } from "../ui";
import { MessageTypeToggle } from "./MessageTypeToggle";
import { MessageTypeSummary } from "./MessageTypeSummary";
import { MessageTypesChart } from "./MessageTypesChart";
import { MessageTypeParticipantsTable } from "./MessageTypeParticipantsTable";
import type {
  MessageTypeDetail,
  MessageTypeKind,
  MessageTypeOverview,
  TypeTimeView,
} from "../../lib/types";
import { MESSAGE_TYPE_LABELS } from "../../lib/messageTypeLabels";
import styles from "./MessageTypesBlock.module.css";

interface MessageTypesBlockProps {
  chatId: number;
  onReimport: () => void;
}

// Та же логика, что и в Dashboard.tsx — не бить invoke на каждый
// промежуточный chatId при быстром переключении чатов в рельсе.
const SWITCH_DEBOUNCE_MS = 150;

// Универсальный блок «Типы сообщений» (макет «Дашборд», ход t6) — секция
// 03 дашборда. 12 типов на этот срез, без реакций (docs/PLAN.md — своя
// таблица, отдельный будущий срез).
export function MessageTypesBlock({ chatId, onReimport }: MessageTypesBlockProps) {
  const [overview, setOverview] = useState<MessageTypeOverview | null>(null);
  const [selectedKind, setSelectedKind] = useState<MessageTypeKind | null>(null);
  // `null` — фронт ещё не высказал предпочтение по гранулярности,
  // бэкенд сам выбирает разумный дефолт (message_types.rs). Меняется
  // только по клику пользователя на Segmented — не пересчитывается при
  // каждом ответе, иначе смена типа сбрасывала бы выбор пользователя.
  const [requestedView, setRequestedView] = useState<TypeTimeView | null>(null);
  const [detail, setDetail] = useState<MessageTypeDetail | null>(null);
  // Пока грузится новый detail, старый остаётся на экране (меньше
  // мигания при переключении типа/вида) — этот флаг только подсвечивает
  // карточку как "обновляется", не прячет её содержимое.
  const [detailLoading, setDetailLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setOverview(null);
    setSelectedKind(null);
    setRequestedView(null);
    setDetail(null);
    const timer = setTimeout(() => {
      invoke<MessageTypeOverview>("get_message_type_overview", { chatId }).then((r) => {
        if (cancelled) return;
        setOverview(r);
        const top = [...r.counts].filter((c) => c.count > 0).sort((a, b) => b.count - a.count)[0];
        if (top) setSelectedKind(top.kind);
      });
    }, SWITCH_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [chatId]);

  useEffect(() => {
    if (selectedKind == null) return;
    let cancelled = false;
    setDetailLoading(true);
    invoke<MessageTypeDetail | null>("get_message_type_detail", {
      chatId,
      kind: selectedKind,
      view: requestedView,
    }).then((r) => {
      if (cancelled) return;
      setDetail(r);
      setDetailLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [chatId, selectedKind, requestedView]);

  function handleSelectKind(kind: MessageTypeKind) {
    setSelectedKind(kind);
    setRequestedView(null);
  }

  if (overview == null) {
    return (
      <div className={styles.wrap} aria-busy="true">
        <Skeleton height={32} width="70%" />
        <Card>
          <Skeleton height={140} />
        </Card>
      </div>
    );
  }

  if (!overview.available) {
    return (
      <UnavailableCard
        title="Типы сообщений"
        message="Файл был импортирован до появления этой метрики — типы сообщений ещё не посчитаны. Переимпортируйте чат, чтобы увидеть разбивку."
        actionLabel="Обновить выгрузку"
        onAction={onReimport}
      />
    );
  }

  if (overview.counts.every((c) => c.count === 0)) {
    return <div className={styles.empty}>В этом чате нет данных о типах сообщений</div>;
  }

  return (
    <div className={styles.wrap}>
      <MessageTypeToggle counts={overview.counts} selected={selectedKind} onSelect={handleSelectKind} />
      {detail == null && detailLoading && (
        <Card>
          <Skeleton height={140} />
        </Card>
      )}
      {detail && (
        <Card
          className={detailLoading ? styles.updating : undefined}
          aria-busy={detailLoading || undefined}
        >
          <MessageTypeSummary
            kind={detail.kind}
            totalCount={detail.total_count}
            pctOfAll={detail.pct_of_all}
            summary={detail.summary}
          />
          <MessageTypesChart
            buckets={detail.buckets}
            view={detail.selected_view}
            viewsAvailable={detail.views_available}
            onViewChange={setRequestedView}
          />
          <MessageTypeParticipantsTable
            participants={detail.participants}
            typeColumnHeader={MESSAGE_TYPE_LABELS[detail.kind].genitive}
          />
        </Card>
      )}
    </div>
  );
}
