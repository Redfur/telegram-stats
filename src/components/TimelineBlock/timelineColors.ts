const SERIES_COUNT = 8;

// Цвет на участника — по позиции в уже отсортированном (по дате первого
// появления) списке строк, не по person_id: соседние строки получают
// соседние (различимые) слоты палитры чаще, чем при хэшировании id.
// Цвет остаётся у всех, включая вышедших/удалённых участников — не
// гасится (подтверждено пользователем при планировании этого среза).
export function seriesColorVar(rowIndex: number): string {
  const safeIndex = ((rowIndex % SERIES_COUNT) + SERIES_COUNT) % SERIES_COUNT;
  return `var(--series-${safeIndex + 1})`;
}
