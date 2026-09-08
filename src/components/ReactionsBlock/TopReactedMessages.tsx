import { Card, CardKicker } from "../ui";
import { formatFullDateTime, formatShortDate } from "../../lib/formatMessageTimestamp";
import type { TopReactedMessage } from "../../lib/types";
import styles from "./ReactionsBlock.module.css";

interface TopReactedMessagesProps {
  messages: TopReactedMessage[];
}

// Превью снято во время импорта (message_reaction_previews) — `messages`
// сам текст не хранит, поэтому это единственный источник содержимого.
// Для сообщений без текста (фото/голосовое/etc без подписи) бэкенд отдаёт
// content_label вместо пустой строки. Дата — как есть, без предположений
// о часовом поясе (тот же принцип, что и у графика активности), полная
// дата-время — в title на наведение, чтобы искать сообщение в Telegram.
export function TopReactedMessages({ messages }: TopReactedMessagesProps) {
  return (
    <Card>
      <CardKicker>Самые залайканные сообщения</CardKicker>
      <ol className={styles.messageList}>
        {messages.map((m) => (
          <li key={m.message_id} className={styles.messageRow}>
            <span className={styles.messagePreview}>
              {m.preview_text || m.content_label || "—"}
            </span>
            <div className={styles.messageMeta}>
              <span className={styles.messageAuthor}>{m.display_name || "Без имени"}</span>
              {m.preview_text && m.content_label && (
                <span className={styles.messageAuthor}>{m.content_label}</span>
              )}
              {m.ts != null && (
                <span className={styles.messageDate} title={formatFullDateTime(m.ts)}>
                  {formatShortDate(m.ts)}
                </span>
              )}
              <span className="num">{m.total_count.toLocaleString("ru-RU")}</span>
            </div>
          </li>
        ))}
      </ol>
    </Card>
  );
}
