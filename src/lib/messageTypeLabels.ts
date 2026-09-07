import type { MessageTypeKind } from "./types";

// 12 типов — фиксированный словарь Telegram-экспорта, не пользовательские
// данные, поэтому формы для каждого прописаны явно (докстрока в
// MessageTypeKind, message_types.rs).
interface MessageTypeLabel {
  /** Именительный падеж, множественное число — подпись пилюли-переключателя. */
  plural: string;
  /** Родительный падеж, множественное число — «Всего …», заголовок колонки. */
  genitive: string;
}

export const MESSAGE_TYPE_LABELS: Record<MessageTypeKind, MessageTypeLabel> = {
  photo: { plural: "Фото", genitive: "фото" },
  sticker: { plural: "Стикеры", genitive: "стикеров" },
  voice_message: { plural: "Голосовые", genitive: "голосовых" },
  video_message: { plural: "Кружочки", genitive: "кружочков" },
  video_file: { plural: "Видео", genitive: "видео" },
  animation: { plural: "Гифки", genitive: "гифок" },
  file: { plural: "Файлы", genitive: "файлов" },
  forwarded: { plural: "Пересланные", genitive: "пересланных" },
  edited: { plural: "Отредактированные", genitive: "отредактированных" },
  poll: { plural: "Опросы", genitive: "опросов" },
  location: { plural: "Локации", genitive: "локаций" },
  contact: { plural: "Контакты", genitive: "контактов" },
};
