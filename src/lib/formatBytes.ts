// null/0 — величина не показывается вовсе (не «0 Б»), см. docs/PLAN.md:
// source_file_size неизвестен для части путей ещё до этого среза.
export function formatBytes(bytes: number | null | undefined): string | null {
  if (!bytes || bytes <= 0) return null;
  const units: [number, string][] = [
    [1024 ** 3, "ГБ"],
    [1024 ** 2, "МБ"],
    [1024, "КБ"],
  ];
  for (const [threshold, unit] of units) {
    if (bytes >= threshold) {
      return `${(bytes / threshold).toLocaleString("ru-RU", { maximumFractionDigits: 1 })} ${unit}`;
    }
  }
  return `${bytes} Б`;
}
