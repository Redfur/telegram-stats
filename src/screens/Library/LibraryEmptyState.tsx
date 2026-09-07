import type { ReactNode } from "react";
import { ImportDropzone } from "../../components/ImportDropzone/ImportDropzone";
import styles from "./LibraryEmptyState.module.css";

const STEPS: { n: string; text: ReactNode }[] = [
  { n: "01", text: "Откройте Telegram Desktop, нужный чат" },
  { n: "02", text: <>Меню чата → «Экспорт истории чата»</> },
  {
    n: "03",
    text: (
      <>
        Формат — <b>JSON</b>, медиа можно отключить
      </>
    ),
  },
  {
    n: "04",
    text: (
      <>
        Перетащите готовый <code className={styles.code}>result.json</code> сюда
      </>
    ),
  },
];

interface LibraryEmptyStateProps {
  onPickFile: () => void;
  isImporting?: boolean;
  error?: string | null;
}

// Первый запуск — макет «Экраны приложения» опция 3b.
export function LibraryEmptyState({ onPickFile, isImporting, error }: LibraryEmptyStateProps) {
  return (
    <div className={styles.wrap}>
      <div className={styles.intro}>
        <div>
          <span className={["kick", styles.kicker].join(" ")}>ПЕРВЫЙ ЗАПУСК</span>
          <h1 className={styles.heading}>Здесь появятся ваши чаты</h1>
          <p className={styles.lead}>
            Всё считается на этом компьютере. Файл выгрузки никуда не отправляется и остаётся у вас.
          </p>
        </div>
        <div className={styles.steps}>
          {STEPS.map((step) => (
            <div className={styles.step} key={step.n}>
              <span className={["num", styles.stepNum].join(" ")}>{step.n}</span>
              <span className={styles.stepText}>{step.text}</span>
            </div>
          ))}
        </div>
        <div className={styles.footnote}>Большая история выгружается в Telegram несколько минут — это нормально.</div>
      </div>
      <div className={styles.dropzoneCol}>
        <ImportDropzone onPickFile={onPickFile} isImporting={isImporting} error={error} />
      </div>
    </div>
  );
}
