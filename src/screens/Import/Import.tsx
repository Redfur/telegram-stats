import { ImportOnboarding } from "../../components/ImportOnboarding/ImportOnboarding";

interface ImportProps {
  onPickFile: () => void;
  isImporting?: boolean;
  error?: string | null;
}

// Пункт нава «Импорт» (docs/PLAN.md) — та же инструкция, что и на первом
// запуске (LibraryEmptyState), для чата, добавляемого не первым.
export function Import({ onPickFile, isImporting, error }: ImportProps) {
  return (
    <ImportOnboarding
      kicker="ИМПОРТ"
      heading="Добавить ещё один чат"
      lead="Новый импорт не трогает уже сохранённые чаты — они остаются в библиотеке как есть."
      onPickFile={onPickFile}
      isImporting={isImporting}
      error={error}
    />
  );
}
