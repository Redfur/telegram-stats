import { Button } from "../ui";
import { ParticipantsManager } from "../ParticipantsManager/ParticipantsManager";
import type { PersonRow } from "../../lib/types";
import styles from "./ParticipantReviewStep.module.css";

interface ParticipantReviewStepProps {
  people: PersonRow[];
  onMerge: (primaryId: number, otherIds: number[]) => Promise<void>;
  onHide: (personId: number, hidden: boolean) => Promise<void>;
  onRename: (personId: number, displayName: string) => Promise<void>;
  onDone: () => void;
  onSkip: () => void;
}

// Макет «Экраны приложения» 3d, базовый объём (подтверждено с
// пользователем): переименование/скрытие/объединение дублей, без
// авто-детекта дублей/ботов, сегмент-фильтра и сворачивания хвоста —
// для них нет честного сигнала в данных на этом этапе (см. docs/PLAN.md).
// Таблица и панель действий — общий ParticipantsManager, здесь только
// chrome мастера импорта (шаг 3 из 3, кнопки «Готово»/«Пропустить»).
export function ParticipantReviewStep({ people, onMerge, onHide, onRename, onDone, onSkip }: ParticipantReviewStepProps) {
  return (
    <div className={styles.overlay}>
      <div className={styles.panel}>
        <div className={styles.headRow}>
          <div>
            <div className={["kick", styles.kicker].join(" ")}>ИМПОРТ · ШАГ 3 ИЗ 3</div>
            <h3 className={styles.title}>Участники</h3>
            <div className={styles.subtitle}>{people.length.toLocaleString("ru-RU")} найдено</div>
          </div>
          <Button variant="primary" onClick={onDone} className={styles.doneBtn}>
            Готово, к дашборду
          </Button>
        </div>

        <ParticipantsManager
          people={people}
          onMerge={onMerge}
          onHide={onHide}
          onRename={onRename}
          footerRight={
            <span className={styles.skip} onClick={onSkip}>
              Пропустить разбор
            </span>
          }
        />
      </div>
    </div>
  );
}
