import type { TypeTimeView } from "./types";

// Пн-первый порядок — bucket "0" от бэкенда уже понедельник, не
// воскресенье (страховка от "сырого" strftime('%w') сделана в SQL,
// message_types.rs), здесь только подпись по индексу.
const WEEKDAY_LABELS = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];

// Подпись бакета блока «Типы сообщений» — три вида, не пять, как у
// formatBucketLabel/Granularity (см. TypeTimeView в message_types.rs).
// day_of_week/hour_of_day — распределения по всему периоду, не даты.
export function formatTypeBucketLabel(bucket: string, view: TypeTimeView): string {
  switch (view) {
    case "month": {
      const [year, month] = bucket.split("-");
      const d = new Date(Date.UTC(Number(year), Number(month) - 1, 1));
      return d.toLocaleDateString("ru-RU", { month: "short", year: "numeric" });
    }
    case "day_of_week":
      return WEEKDAY_LABELS[Number(bucket)] ?? bucket;
    case "hour_of_day":
      return `${bucket}:00`;
  }
}
