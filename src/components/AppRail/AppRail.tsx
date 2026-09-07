import type { ChatSummary } from "../../lib/types";
import styles from "./AppRail.module.css";

export type NavTarget = "library" | "people" | "import" | "slides";

const NAV_ITEMS: { target: NavTarget; label: string }[] = [
  { target: "library", label: "Библиотека" },
  { target: "people", label: "Участники" },
  { target: "import", label: "Импорт" },
  { target: "slides", label: "Слайды" },
];

interface AppRailProps {
  activeNav: NavTarget;
  onNavigate: (target: NavTarget) => void;
  chats: ChatSummary[];
  activeChatId: number | null;
  onSelectChat: (id: number) => void;
}

export function AppRail({ activeNav, onNavigate, chats, activeChatId, onSelectChat }: AppRailProps) {
  return (
    <div className={styles.rail}>
      <div className={styles.brand}>TG&nbsp;/&nbsp;ARCHIVE</div>
      <div className={styles.chatsHeader}>
        <span className="kick">ЧАТЫ</span>
      </div>
      {chats.length === 0 ? (
        <div className={styles.empty}>Пока ничего не импортировано</div>
      ) : (
        <div className={styles.chatList}>
          {chats.map((chat) => (
            <button
              key={chat.id}
              type="button"
              className={[styles.chatItem, chat.id === activeChatId ? styles.active : ""].filter(Boolean).join(" ")}
              aria-current={chat.id === activeChatId ? "page" : undefined}
              onClick={() => onSelectChat(chat.id)}
            >
              <div className={styles.chatName}>{chat.name || "Без названия"}</div>
              <div className={styles.chatMeta}>
                {chat.chat_type} · {chat.message_count.toLocaleString("ru-RU")}
              </div>
            </button>
          ))}
        </div>
      )}
      <nav className={styles.nav}>
        {NAV_ITEMS.map((item) => (
          <button
            key={item.target}
            type="button"
            className={[styles.navItem, item.target === activeNav && activeChatId == null ? styles.active : ""]
              .filter(Boolean)
              .join(" ")}
            aria-current={item.target === activeNav && activeChatId == null ? "page" : undefined}
            onClick={() => onNavigate(item.target)}
          >
            {item.label}
          </button>
        ))}
      </nav>
    </div>
  );
}
