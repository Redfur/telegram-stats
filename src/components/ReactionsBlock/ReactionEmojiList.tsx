import { useState } from "react";
import { Button, CardKicker } from "../ui";
import type { ReactionEmojiCount } from "../../lib/types";
import styles from "./ReactionsBlock.module.css";

interface ReactionEmojiListProps {
  topEmoji: ReactionEmojiCount[];
  totalReactions: number;
  reactedMessageCount: number;
  totalMessages: number;
}

// Список эмодзи может быть длинным (стикерпаки, кастомные реакции) — по
// умолчанию видно топ-10, остальное — под "показать все", тот же приём,
// что и у MessageTypeToggle (VISIBLE_COUNT + разворачивание по клику).
const VISIBLE_COUNT = 10;

// Сводка блока (карточка сверху): две цифры + бар-чарт по типам реакций,
// отсортированным по частоте бэкендом (metrics::reactions). custom_emoji
// не несёт готового глифа (`emoji: null`) — Telegram отдаёт только
// document_id набора стикеров, картинку без сети не достать, поэтому
// вместо неё нейтральная плашка.
export function ReactionEmojiList({
  topEmoji,
  totalReactions,
  reactedMessageCount,
  totalMessages,
}: ReactionEmojiListProps) {
  const [expanded, setExpanded] = useState(false);
  const maxCount = Math.max(1, ...topEmoji.map((e) => e.count));
  const pctReacted = totalMessages > 0 ? (reactedMessageCount / totalMessages) * 100 : 0;
  const visible = expanded ? topEmoji : topEmoji.slice(0, VISIBLE_COUNT);
  const hiddenCount = topEmoji.length - visible.length;

  return (
    <div className={styles.summary}>
      <div className={styles.tiles}>
        <div className={styles.tile}>
          <CardKicker>Всего реакций</CardKicker>
          <div className="num">{totalReactions.toLocaleString("ru-RU")}</div>
        </div>
        <div className={styles.tile}>
          <CardKicker>Сообщений с реакциями</CardKicker>
          <div className="num">
            {reactedMessageCount.toLocaleString("ru-RU")}{" "}
            <span className={styles.pct}>({pctReacted.toFixed(1).replace(".", ",")}%)</span>
          </div>
        </div>
      </div>
      <div className={styles.emojiList}>
        {visible.map((e, i) => (
          <div key={`${e.kind}-${e.emoji ?? e.document_id ?? i}`} className={styles.emojiRow}>
            <span className={styles.emojiGlyph}>{e.emoji ?? "🖼"}</span>
            <div className={styles.barTrack}>
              <div
                className={styles.barFill}
                style={{ width: `${Math.round((e.count / maxCount) * 100)}%` }}
              />
            </div>
            <span className="num">{e.count.toLocaleString("ru-RU")}</span>
          </div>
        ))}
      </div>
      {hiddenCount > 0 && (
        <Button type="button" variant="ghost" className={styles.showAll} onClick={() => setExpanded(true)}>
          показать все ({hiddenCount} ещё) ▾
        </Button>
      )}
    </div>
  );
}
