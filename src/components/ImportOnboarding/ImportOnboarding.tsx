import type { ReactNode } from "react";
import { ImportDropzone } from "../ImportDropzone/ImportDropzone";
import styles from "./ImportOnboarding.module.css";

const STEPS: { n: string; text: ReactNode }[] = [
  { n: "01", text: "Откройте Telegram Desktop, нужный чат" },
  { n: "02", text: <>Меню чата → «Экспорт истории чата»</> },
  {
    n: "03",
    text: (
      <>
        Формат — <b>JSON</b>, медиа (фото/видео/голосовые) — <b>выключите</b>: с ними экспорт
        может идти в разы дольше, а для статистики они не нужны
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

interface ImportOnboardingProps {
  kicker: string;
  heading: string;
  lead: ReactNode;
  onPickFile: () => void;
  isImporting?: boolean;
  error?: string | null;
}

// Инструкция «как выгрузить и куда положить» — общая для первого запуска
// (LibraryEmptyState, макет 3b) и самостоятельного экрана «Импорт»
// (docs/PLAN.md), различаются только заголовок и лид.
export function ImportOnboarding({ kicker, heading, lead, onPickFile, isImporting, error }: ImportOnboardingProps) {
  return (
    <div className={styles.wrap}>
      <div className={styles.intro}>
        <div>
          <span className={["kick", styles.kicker].join(" ")}>{kicker}</span>
          <h1 className={styles.heading}>{heading}</h1>
          <p className={styles.lead}>{lead}</p>
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
