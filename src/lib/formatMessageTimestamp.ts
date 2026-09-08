// Короткая дата для отображения + полная дата-время для тултипа при
// наведении (макет «Реакции» — топ залайканных сообщений). `ts` —
// секунды unixtime, часовой пояс исходного экспорта неизвестен (тот же
// принцип, что и у каveat в разделе "01 АКТИВНОСТЬ" Dashboard.tsx),
// поэтому просто показываем как есть в часовом поясе просмотра, без
// предположений.
const SHORT_DATE = new Intl.DateTimeFormat("ru-RU", { day: "2-digit", month: "2-digit", year: "numeric" });
const FULL_DATE_TIME = new Intl.DateTimeFormat("ru-RU", {
  day: "2-digit",
  month: "long",
  year: "numeric",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
});

export function formatShortDate(ts: number): string {
  return SHORT_DATE.format(new Date(ts * 1000));
}

export function formatFullDateTime(ts: number): string {
  return FULL_DATE_TIME.format(new Date(ts * 1000));
}
