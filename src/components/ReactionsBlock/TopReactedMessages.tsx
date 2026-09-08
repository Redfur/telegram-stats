import { openUrl } from "@tauri-apps/plugin-opener";
import { CardKicker } from "../ui";
import type { TopReactedMessage } from "../../lib/types";
import styles from "./ReactionsBlock.module.css";

interface TopReactedMessagesProps {
  messages: TopReactedMessage[];
  chatType: string;
  tgChatId: number | null;
}

// Только супергруппы/каналы имеют постоянную ссылку вида
// t.me/c/<id>/<msg> — у личных чатов и обычных ("basic") групп такой
// ссылки не существует в принципе, не только в нашей БД.
function canDeepLink(chatType: string, tgChatId: number | null): boolean {
  return tgChatId != null && (chatType.includes("supergroup") || chatType.includes("channel"));
}

// «Самые залайканные» (макет «Дашборд», ход t8/8a) — ранг + одна строка
// текста с многоточием + автор. Превью снято во время импорта
// (message_reaction_previews) — `messages` сам текст не хранит;
// content_label — фолбэк для сообщений без подписи (фото/голосовое/etc).
// Клик открывает сообщение в Telegram внешней ссылкой (openUrl —
// системный обработчик t.me-ссылок, не сетевой вызов из приложения).
export function TopReactedMessages({ messages, chatType, tgChatId }: TopReactedMessagesProps) {
  const linkable = canDeepLink(chatType, tgChatId);

  return (
    <div>
      <CardKicker>Самые залайканные</CardKicker>
      <ol className={styles.messageList}>
        {messages.map((m, i) => {
          const text = m.preview_text || m.content_label || "—";
          const href = linkable ? `https://t.me/c/${tgChatId}/${m.tg_msg_id}` : null;
          const content = (
            <>
              <span className={styles.messageRank}>{i + 1}</span>
              <span className={styles.messagePreview}>{text}</span>
              <span className={styles.messageAuthor}>{m.display_name || "Без имени"}</span>
            </>
          );
          return (
            <li key={m.message_id} className={styles.messageRow}>
              {href ? (
                <button
                  type="button"
                  className={styles.messageLink}
                  onClick={() => void openUrl(href)}
                  title="Открыть сообщение в Telegram"
                >
                  {content}
                </button>
              ) : (
                <div className={styles.messageLink}>{content}</div>
              )}
            </li>
          );
        })}
      </ol>
    </div>
  );
}
