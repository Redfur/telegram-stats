import { Card, CardKicker } from "../Card/Card";
import { Button } from "../Button/Button";
import styles from "./UnavailableCard.module.css";

interface UnavailableCardProps {
  title: string;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
}

// Паттерн "метрика недоступна" (макет «Дашборд», ход 6c: старая версия
// экспорта без нужного поля) — общий для любой метрики с опциональными
// требованиями к данным (docs/SPEC.md §7), не только «Типы сообщений».
export function UnavailableCard({ title, message, actionLabel, onAction }: UnavailableCardProps) {
  return (
    <Card className={styles.card}>
      <CardKicker>{title}</CardKicker>
      <p className={styles.message}>{message}</p>
      {actionLabel && onAction && (
        <Button variant="secondary" onClick={onAction} className={styles.action}>
          {actionLabel}
        </Button>
      )}
    </Card>
  );
}
