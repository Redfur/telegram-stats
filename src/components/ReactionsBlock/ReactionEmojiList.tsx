import { CardKicker } from "../ui";
import type { ReactionEmojiCount } from "../../lib/types";
import styles from "./ReactionsBlock.module.css";

interface ReactionEmojiListProps {
  topEmoji: ReactionEmojiCount[];
  totalReactions: number;
  reactedMessageCount: number;
  totalMessages: number;
}

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
  const maxCount = Math.max(1, ...topEmoji.map((e) => e.count));
  const pctReacted = totalMessages > 0 ? (reactedMessageCount / totalMessages) * 100 : 0;

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
        {topEmoji.map((e, i) => (
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
    </div>
  );
}
