import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Skeleton, UnavailableCard } from "../ui";
import { ReactionEmojiList } from "./ReactionEmojiList";
import { ReactionPersonLists } from "./ReactionPersonLists";
import { TopReactedMessages } from "./TopReactedMessages";
import type { ReactionsOverview, TopReactedMessage } from "../../lib/types";
import styles from "./ReactionsBlock.module.css";

interface ReactionsBlockProps {
  chatId: number;
  onReimport: () => void;
}

// Та же логика, что и в Dashboard.tsx/MessageTypesBlock.tsx — не бить
// invoke на каждый промежуточный chatId при быстром переключении чатов.
const SWITCH_DEBOUNCE_MS = 150;

// Блок «Реакции» (docs/SPEC.md §9 «Социальная динамика», первый срез —
// агрегаты + given/received по персонам, без "кто чей фанат" и без
// разбивки по времени, см. docs/PLAN.md). В отличие от «Типов сообщений»
// у недоступности метрики два разных исхода — see ReactionsOverview в
// lib/types.ts — с разным сообщением и разным (или отсутствующим) CTA.
export function ReactionsBlock({ chatId, onReimport }: ReactionsBlockProps) {
  const [overview, setOverview] = useState<ReactionsOverview | null>(null);
  const [topMessages, setTopMessages] = useState<TopReactedMessage[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    setOverview(null);
    setTopMessages(null);
    const timer = setTimeout(() => {
      invoke<ReactionsOverview>("get_reactions_overview", { chatId }).then((r) => {
        if (cancelled) return;
        setOverview(r);
        if (r.status === "available") {
          invoke<TopReactedMessage[]>("get_top_reacted_messages", { chatId }).then((msgs) => {
            if (!cancelled) setTopMessages(msgs);
          });
        }
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
          <Skeleton height={140} />
        </Card>
      </div>
    );
  }

  if (overview.status === "needs_reimport") {
    return (
      <UnavailableCard
        title="Реакции"
        message="Файл был импортирован до появления этой метрики — реакции ещё не сохранены. Переимпортируйте чат, чтобы увидеть разбивку."
        actionLabel="Обновить выгрузку"
        onAction={onReimport}
      />
    );
  }

  if (overview.status === "not_in_export") {
    return (
      <UnavailableCard
        title="Реакции"
        message="Этот экспорт сделан старой версией Telegram Desktop и не содержит данных о реакциях. Чтобы увидеть эту метрику, выгрузите чат заново в актуальной версии приложения."
      />
    );
  }

  if (overview.total_reactions === 0) {
    return <div className={styles.empty}>В этом чате никто не ставил реакции</div>;
  }

  return (
    <div className={styles.wrap}>
      <Card>
        <ReactionEmojiList
          topEmoji={overview.top_emoji}
          totalReactions={overview.total_reactions}
          reactedMessageCount={overview.reacted_message_count}
          totalMessages={overview.total_messages}
        />
      </Card>
      <ReactionPersonLists
        given={overview.given}
        received={overview.received}
        givenDataIncomplete={overview.given_data_incomplete}
      />
      {topMessages && topMessages.length > 0 && <TopReactedMessages messages={topMessages} />}
    </div>
  );
}
