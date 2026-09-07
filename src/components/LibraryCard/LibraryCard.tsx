import { Button, Card, Tag, ConfirmDialog } from "../ui";
import type { ChatSummary } from "../../lib/types";
import { formatPeriod } from "../../lib/formatPeriod";
import styles from "./LibraryCard.module.css";

const UPDATED_AT = new Intl.DateTimeFormat("ru-RU", { day: "numeric", month: "long", year: "numeric" });

interface LibraryCardProps {
  chat: ChatSummary;
  onOpen: (id: number) => void;
  onReimport: () => void;
  onDelete: (id: number) => void;
}

// Карточка библиотеки — макет «Экраны приложения» 3a. Без вырождения в
// соотношение для чатов на двоих (это остаётся особенностью дашборда) и
// без строки «доступно N метрик из M» — фикция, пока нет системы
// объявления требований метрик (см. docs/PLAN.md).
export function LibraryCard({ chat, onOpen, onReimport, onDelete }: LibraryCardProps) {
  const period = formatPeriod(chat.first_msg_ts, chat.last_msg_ts);

  return (
    <Card className={styles.card}>
      <div className={styles.head}>
        <Tag variant={chat.participant_count > 2 ? "accent" : "neutral"}>{chat.chat_type}</Tag>
        <span className={styles.updated}>обновлён {UPDATED_AT.format(new Date(chat.imported_at * 1000))}</span>
      </div>
      <div className={styles.title}>{chat.name || "Без названия"}</div>
      {period && <div className={["num", styles.period].join(" ")}>{period}</div>}
      <div className={styles.stats}>
        <div className={styles.stat}>
          <div className={styles.statLabel}>Сообщений</div>
          <div className={["num", styles.statValue].join(" ")}>
            {chat.message_count.toLocaleString("ru-RU")}
          </div>
        </div>
        <div className={styles.stat}>
          <div className={styles.statLabel}>Участников</div>
          <div className={["num", styles.statValue].join(" ")}>
            {chat.participant_count.toLocaleString("ru-RU")}
          </div>
        </div>
      </div>
      <div className={styles.actions}>
        <Button variant="primary" onClick={() => onOpen(chat.id)}>
          Открыть
        </Button>
        <Button variant="secondary" onClick={onReimport}>
          Обновить выгрузкой
        </Button>
        <ConfirmDialog
          trigger={
            <Button variant="ghost" className={styles.deleteBtn}>
              Удалить
            </Button>
          }
          title={`Удалить «${chat.name || "Без названия"}»?`}
          description="Удалятся все сообщения и участники этого чата. Исходный файл выгрузки не пострадает — импорт можно повторить."
          confirmLabel="Удалить"
          onConfirm={() => onDelete(chat.id)}
        />
      </div>
    </Card>
  );
}
