import { Button } from "../../components/ui";
import { LibraryCard } from "../../components/LibraryCard/LibraryCard";
import { formatBytes } from "../../lib/formatBytes";
import type { ChatSummary } from "../../lib/types";
import styles from "./Library.module.css";

interface LibraryProps {
  chats: ChatSummary[];
  onImport: () => void;
  isImporting?: boolean;
  error?: string | null;
  onSelectChat: (id: number) => void;
  onDelete: (id: number) => void;
}

// Заполненное состояние — макет «Экраны приложения» 3a. Пустая
// библиотека остаётся LibraryEmptyState (макет 3b), это её продолжение.
export function Library({ chats, onImport, isImporting, error, onSelectChat, onDelete }: LibraryProps) {
  const totalMessages = chats.reduce((sum, c) => sum + c.message_count, 0);
  const totalSize = chats.reduce((sum, c) => sum + (c.source_file_size ?? 0), 0);
  const sizeLabel = formatBytes(totalSize);

  return (
    <div className={styles.wrap}>
      <div className={styles.headRow}>
        <div>
          <h1 className={styles.title}>Библиотека</h1>
          <div className={styles.summary}>
            {chats.length.toLocaleString("ru-RU")} импорт(ов) · {totalMessages.toLocaleString("ru-RU")} сообщений
            {sizeLabel ? ` · ${sizeLabel} на диске` : ""}
          </div>
        </div>
        <Button variant="primary" onClick={onImport} disabled={isImporting} className={styles.importBtn}>
          {isImporting ? "Импортируется…" : "Импортировать чат"}
        </Button>
      </div>
      {error && <div className={styles.error}>{error}</div>}
      <div className={styles.grid}>
        {chats.map((chat) => (
          <LibraryCard key={chat.id} chat={chat} onOpen={onSelectChat} onReimport={onImport} onDelete={onDelete} />
        ))}
      </div>
    </div>
  );
}
