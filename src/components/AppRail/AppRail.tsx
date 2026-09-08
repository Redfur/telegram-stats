import { NavLink, useMatch } from "react-router-dom";
import type { ChatSummary } from "../../lib/types";
import { useAppVersion } from "../../hooks/useAppVersion";
import styles from "./AppRail.module.css";

function navClassName({ isActive }: { isActive: boolean }) {
  return [styles.navItem, isActive ? styles.active : ""].filter(Boolean).join(" ");
}

interface AppRailProps {
  chats: ChatSummary[];
}

export function AppRail({ chats }: AppRailProps) {
  // Чат сейчас открыт (дашборд или его же «Участники») — только тогда
  // пункт «Участники» вообще куда-то ведёт, см. docs/PLAN.md.
  const chatMatch = useMatch("/chats/:chatId/*");
  const openChatId = chatMatch?.params.chatId ?? null;
  const version = useAppVersion();

  return (
    <div className={styles.rail}>
      <div className={styles.brand}>TG&nbsp;/&nbsp;STATS</div>
      <div className={styles.chatsHeader}>
        <span className="kick">ЧАТЫ</span>
      </div>
      {chats.length === 0 ? (
        <div className={styles.empty}>Пока ничего не импортировано</div>
      ) : (
        <div className={styles.chatList}>
          {chats.map((chat) => (
            <NavLink
              key={chat.id}
              to={`/chats/${chat.id}`}
              className={({ isActive }) => [styles.chatItem, isActive ? styles.active : ""].filter(Boolean).join(" ")}
            >
              <div className={styles.chatName}>{chat.name || "Без названия"}</div>
              <div className={styles.chatMeta}>
                {chat.chat_type} · {chat.message_count.toLocaleString("ru-RU")}
              </div>
            </NavLink>
          ))}
        </div>
      )}
      <nav className={styles.nav}>
        <NavLink to="/" end className={navClassName}>
          Библиотека
        </NavLink>
        {openChatId != null ? (
          <NavLink to={`/chats/${openChatId}/participants`} className={navClassName}>
            Участники
          </NavLink>
        ) : (
          <span className={[styles.navItem, styles.disabled].join(" ")} title="Сначала откройте чат слева">
            Участники
          </span>
        )}
        <NavLink to="/import" className={navClassName}>
          Импорт
        </NavLink>
        <NavLink to="/slides" className={navClassName}>
          Слайды
        </NavLink>
      </nav>
      {version && <div className={styles.version}>v{version}</div>}
    </div>
  );
}
