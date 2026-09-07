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
