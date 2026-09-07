import { Button } from "../ui";
import type { ImportErrorInfo, ImportErrorKind } from "../../hooks/useImportChat";
import styles from "./ImportFlow.module.css";

// Текст подстроен под то, что реально проверяет наш парсер (поле
// `messages` в корне single-chat экспорта) — не дословный перевод
// макета 3c, который упоминает `chats`, поле полного экспорта аккаунта,
// который это приложение не поддерживает.
const COPY: Record<Exclude<ImportErrorKind, "cancelled">, { title: string; body: string; hints?: string[] }> = {
  not_telegram_export: {
    title: "Это не похоже на экспорт Telegram",
    body: "В файле нет поля messages — так в экспорте чата помечается список сообщений.",
    hints: [
      "Формат выгрузки должен быть JSON, не HTML",
      "Нужен файл result.json из корня папки экспорта одного чата",
      "Экспорт делается из Telegram Desktop: меню чата → «Экспорт истории чата»",
    ],
  },
  invalid_json: {
    title: "Файл повреждён или это не JSON",
    body: "Не удалось разобрать содержимое файла как JSON.",
  },
  message_error: {
    title: "Ошибка внутри экспорта",
    body: "Один из элементов списка сообщений не в ожидаемом формате.",
  },
  io: {
    title: "Не удалось прочитать файл",
    body: "",
  },
};

interface ImportErrorCardProps {
  error: ImportErrorInfo;
  onPickAnother: () => void;
  onDismiss: () => void;
}

export function ImportErrorCard({ error, onPickAnother, onDismiss }: ImportErrorCardProps) {
  const copy = COPY[error.kind as Exclude<ImportErrorKind, "cancelled">] ?? COPY.io;

  return (
    <div className={styles.card}>
      <div className={["kick", styles.kickerMuted].join(" ")}>ИМПОРТ · ОШИБКА</div>
      <h4 className={styles.errTitle}>{copy.title}</h4>
      <div className={styles.errBody}>{copy.body || error.message}</div>
      {copy.hints && (
        <div className={styles.hints}>
          {copy.hints.map((h) => (
            <span key={h}>· {h}</span>
          ))}
        </div>
      )}
      <div className={styles.actions}>
        <Button variant="primary" onClick={onPickAnother}>
          Выбрать другой файл
        </Button>
        <Button variant="secondary" onClick={onDismiss}>
          Закрыть
        </Button>
      </div>
      <div className={styles.reassure}>Файл остался на месте — ничего не изменено и не удалено.</div>
    </div>
  );
}
