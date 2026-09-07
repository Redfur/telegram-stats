import type { RateGranularity } from "./types";

// Подпись бакета "пика" в карточке сводки — всегда абсолютная календарная
// дата (месяц или день), не путать с formatTypeBucketLabel: та форматирует
// то, что сейчас выбрано для гистограммы (в т.ч. распределения без даты).
export function formatRateBucketLabel(bucket: string, granularity: RateGranularity): string {
  if (granularity === "month") {
    const [year, month] = bucket.split("-");
    const d = new Date(Date.UTC(Number(year), Number(month) - 1, 1));
    return d.toLocaleDateString("ru-RU", { month: "short", year: "numeric" });
  }
  const d = new Date(bucket + "T00:00:00Z");
  return d.toLocaleDateString("ru-RU", { day: "numeric", month: "short" });
}
