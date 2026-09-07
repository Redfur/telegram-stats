import type { TypeTimeView } from "./types";

// Подпись бакета блока «Типы сообщений» — три вида, не пять, как у
// formatBucketLabel/Granularity (см. TypeTimeView в message_types.rs).
// hour_of_day — час суток (0-23), не абсолютное время, поэтому не дата.
export function formatTypeBucketLabel(bucket: string, view: TypeTimeView): string {
  switch (view) {
    case "month": {
      const [year, month] = bucket.split("-");
      const d = new Date(Date.UTC(Number(year), Number(month) - 1, 1));
      return d.toLocaleDateString("ru-RU", { month: "short", year: "numeric" });
    }
    case "day": {
      const d = new Date(bucket + "T00:00:00Z");
      return d.toLocaleDateString("ru-RU", { day: "numeric", month: "short" });
    }
    case "hour_of_day":
      return `${bucket}:00`;
  }
}
