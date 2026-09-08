import { openUrl } from "@tauri-apps/plugin-opener";
import { CardKicker } from "../ui";
import { formatFullDateTime, formatShortDate } from "../../lib/formatMessageTimestamp";
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

// «Самые залайканные» (макет «Дашборд», ход t8/8a, обновлён — рядом с
// текстом список поставленных реакций): ранг + текст (до 3 строк,
// многоточие на переполнении) + автор, дата и реакции — в одной мета-
// строке. Превью снято во время импорта (message_reaction_previews) —
// `messages` сам текст не хранит; content_label — фолбэк для сообщений
// без подписи. Клик открывает сообщение в Telegram внешней ссылкой
// (openUrl — системный обработчик t.me-ссылок, не сетевой вызов из
// приложения).
export function TopReactedMessages({ messages, chatType, tgChatId }: TopReactedMessagesProps) {
  const linkable = canDeepLink(chatType, tgChatId);

  return (
    <div>
      <CardKicker>Самые залайканные</CardKicker>
      <ol className={styles.messageList}>
        {messages.map((m, i) => {
          const text = m.preview_text || m.content_label || "—";
          const href = linkable ? `https://t.me/c/${tgChatId}/${m.tg_msg_id}` : null;
          const body = (
            <>
              <div className={styles.messagePreview}>{text}</div>
              <div className={styles.messageMeta}>
                <span className={styles.messageAuthor}>{m.display_name || "Без имени"}</span>
                {m.ts != null && (
                  <>
                    <span className={styles.messageMetaSep} />
                    <span className={styles.messageDate} title={formatFullDateTime(m.ts)}>
                      {formatShortDate(m.ts)}
                    </span>
                  </>
                )}
                {m.reactions.length > 0 && <span className={styles.messageMetaSep} />}
                {m.reactions.map((r, ri) => (
                  <span key={`${r.kind}-${r.emoji ?? r.document_id ?? ri}`} className={styles.messageReactionPill}>
                    <span className={styles.emojiGlyph}>{r.emoji ?? "🖼"}</span>
                    {r.count.toLocaleString("ru-RU")}
                  </span>
                ))}
              </div>
            </>
          );
          return (
            <li key={m.message_id} className={styles.messageRow}>
              <span className={styles.messageRank}>{i + 1}</span>
              <div className={styles.messageBody}>
                {href ? (
                  <button
                    type="button"
                    className={styles.messageLink}
                    onClick={() => void openUrl(href)}
                    title="Открыть сообщение в Telegram"
                  >
                    {body}
                  </button>
                ) : (
                  <div className={styles.messageLink}>{body}</div>
                )}
              </div>
            </li>
          );
        })}
      </ol>
    </div>
  );
}
