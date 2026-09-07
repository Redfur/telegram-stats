import { Button, CornerMarks } from "../ui";
import styles from "./ImportDropzone.module.css";

interface ImportDropzoneProps {
  onPickFile?: () => void;
}

// Только визуальная часть — открытие файлового диалога и сам импорт
// (Tauri-команда, стриминговый парсер) относятся к Фазе 1.
export function ImportDropzone({ onPickFile }: ImportDropzoneProps) {
  return (
    <div className={styles.dropzone}>
      <CornerMarks />
      <div className={styles.iconBox}>
        <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" strokeWidth={1.5}>
          <path d="M12 17V4" />
          <path d="M7 9l5-5 5 5" />
          <path d="M4 17v3h16v-3" />
        </svg>
      </div>
      <div className={styles.text}>
        <div className={styles.heading}>Перетащите result.json</div>
        <div className={styles.sub}>или папку с выгрузкой · до 8 ГБ</div>
      </div>
      <Button variant="primary" onClick={onPickFile}>
        Выбрать файл
      </Button>
    </div>
  );
}
