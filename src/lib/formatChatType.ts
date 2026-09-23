// Значения chat_type, реально встречающиеся в экспорте Telegram и в
// фикстурах проекта (docs/SPEC.md §8, tests/fixtures/). Супергруппа —
// внутренняя деталь Telegram, не смысловое отличие для пользователя,
// поэтому private_group/private_supergroup делят одну подпись; публичная
// супергруппа — по смыслу обсуждение, а не вещание, поэтому у неё своя
// подпись, отдельная от канала.
const CHAT_TYPE_LABELS: Record<string, string> = {
  personal_chat: "личная переписка",
  bot_chat: "чат с ботом",
  private_group: "группа",
  private_supergroup: "группа",
  public_supergroup: "публичная группа",
  private_channel: "канал",
  public_channel: "публичный канал",
  saved_messages: "избранное",
};

// Неизвестный будущий тип — как есть, не падаем и не скрываем бейдж.
export function formatChatType(chatType: string): string {
  return CHAT_TYPE_LABELS[chatType] ?? chatType;
}
