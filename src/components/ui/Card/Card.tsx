import type { HTMLAttributes } from "react";
import { CornerMarks } from "../CornerMarks/CornerMarks";
import styles from "./Card.module.css";

export function Card({ className, children, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={[styles.card, className].filter(Boolean).join(" ")} {...rest}>
      <CornerMarks />
      {children}
    </div>
  );
}

export function CardKicker({ className, ...rest }: HTMLAttributes<HTMLSpanElement>) {
  return <span className={[styles.kicker, className].filter(Boolean).join(" ")} {...rest} />;
}
