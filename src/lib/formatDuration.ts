// Единица подбирается по величине самого значения (секунды/минуты/часы),
// не зашита заранее — суммарная длительность обычно в часах, медиана
// одного голосового — в секундах, это разные вызовы одной функции.
export function formatDuration(totalSeconds: number): string {
  const seconds = Math.round(totalSeconds);
  if (seconds < 60) return `${seconds} с`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} мин`;
  const hours = Math.round(seconds / 3600);
  return `${hours} ч`;
}

// Человекочитаемая длительность в днях — «Таймлайн участников» (медианный
// срок пребывания, срок конкретного участника в капсуле при наведении).
// Только формы, реально нужные для сроков членства (дни/месяцы/годы) — не
// общий i18n-пакет.
function pluralize(n: number, one: string, few: string, many: string): string {
  const mod10 = n % 10;
  const mod100 = n % 100;
  if (mod100 >= 11 && mod100 <= 14) return many;
  if (mod10 === 1) return one;
  if (mod10 >= 2 && mod10 <= 4) return few;
  return many;
}

export function formatDurationDays(days: number): string {
  const whole = Math.round(days);
  if (whole < 1) return "меньше дня";
  if (whole < 30) return `${whole} ${pluralize(whole, "день", "дня", "дней")}`;
  if (whole < 365) {
    const months = Math.max(1, Math.round(whole / 30));
    return `${months} ${pluralize(months, "месяц", "месяца", "месяцев")}`;
  }
  const years = Math.floor(whole / 365);
  const remMonths = Math.round((whole - years * 365) / 30);
  const yearsPart = `${years} ${pluralize(years, "год", "года", "лет")}`;
  if (remMonths === 0) return yearsPart;
  return `${yearsPart} ${remMonths} ${pluralize(remMonths, "месяц", "месяца", "месяцев")}`;
}
