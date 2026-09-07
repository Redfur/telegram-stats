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
        {NAV_ITEMS.map((item) => {
          // «Участники» — единственный пункт, который остаётся активным
          // вместе с открытым чатом (макет 3d/3e: чат в списке и пункт
          // нава подсвечены одновременно). Остальные — самостоятельные
          // экраны, открытый чат их подсветку гасит.
          const isActive = item.target === activeNav && (item.target === "people" || activeChatId == null);
          return (
            <button
              key={item.target}
              type="button"
              className={[styles.navItem, isActive ? styles.active : ""].filter(Boolean).join(" ")}
              aria-current={isActive ? "page" : undefined}
              onClick={() => onNavigate(item.target)}
            >
              {item.label}
            </button>
          );
        })}
      </nav>
    </div>
  );
}
