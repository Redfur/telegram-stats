import { ParticipantsManager } from "../../components/ParticipantsManager/ParticipantsManager";
import { usePeopleReview } from "../../hooks/usePeopleReview";
import styles from "./Participants.module.css";

interface ParticipantsProps {
  chatId: number;
  chatName: string | null;
}

// Самостоятельный экран «Участники» вне мастера импорта (docs/PLAN.md,
// Фаза 3) — тот же макет 3d/3e, что и ParticipantReviewStep, но как
// обычная страница (шапка в стиле Library, без модального chrome и без
// «Готово»/«Пропустить», это не шаг мастера).
export function Participants({ chatId, chatName }: ParticipantsProps) {
  const { people, merge, hide, rename } = usePeopleReview(chatId);

  return (
    <div className={styles.wrap}>
      <div className={styles.headRow}>
        <div>
          <h1 className={styles.title}>Участники{chatName ? ` · ${chatName}` : ""}</h1>
          <div className={styles.summary}>{people.length.toLocaleString("ru-RU")} найдено</div>
        </div>
      </div>
      <ParticipantsManager people={people} onMerge={merge} onHide={hide} onRename={rename} />
    </div>
  );
}
