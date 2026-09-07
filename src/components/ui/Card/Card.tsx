import type { HTMLAttributes } from "react";
import styles from "./Card.module.css";

// Регистрационные метки по углам обязательны у обрамлённых объектов
// (см. docs/PLAN.md / макет «Дизайн-токены» §5b) — Card рисует их сама.
export function Card({ className, children, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={[styles.card, className].filter(Boolean).join(" ")} {...rest}>
      <i className={[styles.corner, styles.tl].join(" ")} />
      <i className={[styles.corner, styles.tr].join(" ")} />
      <i className={[styles.corner, styles.bl].join(" ")} />
      <i className={[styles.corner, styles.br].join(" ")} />
      {children}
    </div>
  );
}

export function CardKicker({ className, ...rest }: HTMLAttributes<HTMLSpanElement>) {
  return <span className={[styles.kicker, className].filter(Boolean).join(" ")} {...rest} />;
}
