import { useState } from "react";
import { CardKicker } from "../ui";
import type { ReactionEmojiCount } from "../../lib/types";
import styles from "./ReactionsBlock.module.css";

interface ReactionEmojiListProps {
  topEmoji: ReactionEmojiCount[];
  totalReactions: number;
  reactedMessageCount: number;
  totalMessages: number;
}

// Сетка эмодзи может быть длинной (стикерпаки, кастомные реакции) — по
// умолчанию видно топ-12 (4 колонки × 3 ряда, макет «Дашборд» ход t8/8a),
// остальное — под «показать все», компактными чипами, не полными
// карточками.
const VISIBLE_COUNT = 12;
const TOP_CONCENTRATION_COUNT = 4;

// Сводка блока: 4 показателя (всего реакций + доля сообщений, среднее на
// сообщение с реакцией, число разных эмодзи, концентрация топ-4) — все
// считаются на фронте из уже присланных полей, отдельного бэкенда не
// требуют. custom_emoji не несёт готового глифа (`emoji: null`) —
// Telegram отдаёт только document_id набора стикеров, картинку без сети
// не достать, поэтому вместо неё нейтральная плашка.
export function ReactionEmojiList({
  topEmoji,
  totalReactions,
  reactedMessageCount,
  totalMessages,
}: ReactionEmojiListProps) {
  const [expanded, setExpanded] = useState(false);

  const msgSharePct = totalMessages > 0 ? (reactedMessageCount / totalMessages) * 100 : 0;
  const perMessage = reactedMessageCount > 0 ? totalReactions / reactedMessageCount : 0;
  const top4 = topEmoji.slice(0, TOP_CONCENTRATION_COUNT);
  const top4Count = top4.reduce((sum, e) => sum + e.count, 0);
  const top4Pct = totalReactions > 0 ? (top4Count / totalReactions) * 100 : 0;

  // Первые VISIBLE_COUNT всегда в подробном виде (грид с процентом и
  // полосой); хвост по умолчанию не рендерится вовсе — «показать все»
  // именно раскрывает его компактными чипами, а не превращает в
  // подробные карточки.
  const visible = topEmoji.slice(0, VISIBLE_COUNT);
  const tail = expanded ? topEmoji.slice(VISIBLE_COUNT) : [];
  const maxCount = Math.max(1, ...visible.map((e) => e.count));

  return (
    <div className={styles.summary}>
      <div className={styles.statRow}>
        <div className={styles.statMain}>
          <CardKicker>Всего реакций</CardKicker>
          <div className={styles.statBig}>{totalReactions.toLocaleString("ru-RU")}</div>
          <div className={styles.statCaption}>{msgSharePct.toFixed(0)}% сообщений</div>
        </div>
        <div className={styles.statTiles}>
          <div className={styles.statTile}>
            <div className={styles.statTileLabel}>На сообщение с реакцией</div>
            <div className={styles.statTileValue}>{perMessage.toFixed(1).replace(".", ",")}</div>
          </div>
          <div className={styles.statTile}>
            <div className={styles.statTileLabel}>Разных эмодзи</div>
            <div className={styles.statTileValue}>{topEmoji.length.toLocaleString("ru-RU")}</div>
          </div>
          <div className={styles.statTile}>
            <div className={styles.statTileLabel}>
              Топ-{TOP_CONCENTRATION_COUNT} = {top4Pct.toFixed(0)}%
            </div>
            <div className={styles.statTileGlyphs}>
              {top4.map((e, i) => (
                <span key={`${e.kind}-${e.emoji ?? e.document_id ?? i}`}>{e.emoji ?? "🖼"}</span>
              ))}
            </div>
          </div>
        </div>
      </div>

      <div className={styles.emojiGrid}>
        {visible.map((e, i) => {
          const pct = totalReactions > 0 ? (e.count / totalReactions) * 100 : 0;
          return (
            <div key={`${e.kind}-${e.emoji ?? e.document_id ?? i}`} className={styles.emojiCell}>
              <div className={styles.emojiCellHead}>
                <span className={styles.emojiGlyph}>{e.emoji ?? "🖼"}</span>
                <span className={styles.emojiCellCount}>{e.count.toLocaleString("ru-RU")}</span>
                <span className={styles.emojiCellPct}>{pct.toFixed(0)}%</span>
              </div>
              <div className={styles.barTrack}>
                <div
                  className={styles.barFill}
                  style={{ width: `${Math.round((e.count / maxCount) * 100)}%` }}
                />
              </div>
            </div>
          );
        })}
      </div>

      {tail.length > 0 && (
        <div className={styles.emojiTail}>
          {tail.map((e, i) => (
            <span key={`${e.kind}-${e.emoji ?? e.document_id ?? i}`} className={styles.emojiTailChip}>
              <span className={styles.emojiGlyph}>{e.emoji ?? "🖼"}</span>
              {e.count.toLocaleString("ru-RU")}
            </span>
          ))}
        </div>
      )}

      {topEmoji.length > VISIBLE_COUNT && (
        <button type="button" className={styles.showAll} onClick={() => setExpanded((v) => !v)}>
          {expanded ? "свернуть" : `показать все (ещё ${topEmoji.length - VISIBLE_COUNT}) ▾`}
        </button>
      )}
    </div>
  );
}
