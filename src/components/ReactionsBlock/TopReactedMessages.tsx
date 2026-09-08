import { Card, CardKicker } from "../ui";
import type { TopReactedMessage } from "../../lib/types";
import styles from "./ReactionsBlock.module.css";

interface TopReactedMessagesProps {
  messages: TopReactedMessage[];
}

// Превью снято во время импорта (message_reaction_previews) — `messages`
// сам текст не хранит, поэтому это единственный источник содержимого.
export function TopReactedMessages({ messages }: TopReactedMessagesProps) {
  return (
    <Card>
      <CardKicker>Самые залайканные сообщения</CardKicker>
      <ol className={styles.messageList}>
        {messages.map((m) => (
          <li key={m.message_id} className={styles.messageRow}>
            <span className={styles.messagePreview}>{m.preview_text || "—"}</span>
            <span className={styles.messageAuthor}>{m.display_name || "Без имени"}</span>
            <span className="num">{m.total_count.toLocaleString("ru-RU")}</span>
          </li>
        ))}
      </ol>
    </Card>
  );
}
