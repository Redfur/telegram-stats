import type { Granularity } from "./types";

// Подпись бакета для оси графика. Границы бакетов — забота Rust/SQL
// (metrics::granularity), здесь только форматирование под гранулярность.
export function formatBucketLabel(bucket: string, granularity: Granularity): string {
  switch (granularity) {
    case "hour": {
      const d = new Date(bucket + "Z"); // формат из SQL уже "YYYY-MM-DDTHH:00:00"
      return d.toLocaleString("ru-RU", { day: "2-digit", month: "2-digit", hour: "2-digit" });
    }
    case "day": {
      const d = new Date(bucket + "T00:00:00Z");
      return d.toLocaleDateString("ru-RU", { day: "2-digit", month: "2-digit" });
    }
    case "week":
      // "%Y-%W" из SQLite — год и номер недели, не настоящая дата
      return `нед. ${bucket}`;
    case "month": {
      const [year, month] = bucket.split("-");
      const d = new Date(Date.UTC(Number(year), Number(month) - 1, 1));
      return d.toLocaleDateString("ru-RU", { month: "short", year: "numeric" });
    }
    case "year":
      return bucket;
  }
}
