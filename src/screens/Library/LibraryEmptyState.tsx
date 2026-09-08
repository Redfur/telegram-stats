import { ImportOnboarding } from "../../components/ImportOnboarding/ImportOnboarding";

interface LibraryEmptyStateProps {
  onPickFile: () => void;
  isImporting?: boolean;
  error?: string | null;
}

// Первый запуск — макет «Экраны приложения» опция 3b.
export function LibraryEmptyState({ onPickFile, isImporting, error }: LibraryEmptyStateProps) {
  return (
    <ImportOnboarding
      kicker="ПЕРВЫЙ ЗАПУСК"
      heading="Здесь появятся ваши чаты"
      lead="Всё считается на этом компьютере. Файл выгрузки никуда не отправляется и остаётся у вас."
      onPickFile={onPickFile}
      isImporting={isImporting}
      error={error}
    />
  );
}
