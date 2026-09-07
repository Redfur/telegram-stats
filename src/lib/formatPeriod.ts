const MONTH_YEAR = new Intl.DateTimeFormat("ru-RU", { month: "long", year: "numeric" });

function daysWord(n: number): string {
  const mod100 = n % 100;
  const mod10 = n % 10;
  if (mod100 >= 11 && mod100 <= 14) return "дней";
  if (mod10 === 1) return "день";
  if (mod10 >= 2 && mod10 <= 4) return "дня";
  return "дней";
}

// "март 2015 — декабрь 2025 · 3 957 дней". Обе границы форматируются
// одинаково (без спецкейса «сегодня» из макета — потребовал бы
// произвольного порога свежести).
export function formatPeriod(firstTs: number | null, lastTs: number | null): string | null {
  if (firstTs == null || lastTs == null) return null;
  const from = MONTH_YEAR.format(new Date(firstTs * 1000));
  const to = MONTH_YEAR.format(new Date(lastTs * 1000));
  const days = Math.max(1, Math.round((lastTs - firstTs) / 86400));
  const range = from === to ? from : `${from} — ${to}`;
  return `${range} · ${days.toLocaleString("ru-RU")} ${daysWord(days)}`;
}
