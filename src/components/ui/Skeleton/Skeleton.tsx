import type { CSSProperties } from "react";
import styles from "./Skeleton.module.css";

interface SkeletonProps {
  width?: CSSProperties["width"];
  height?: CSSProperties["height"];
  className?: string;
}

// Плейсхолдер на время загрузки — пульсирующий блок без содержимого.
// Не сообщает читалкам ничего (aria-hidden) — секция сама отвечает за
// доступное состояние загрузки (например через aria-busy на контейнере).
export function Skeleton({ width, height, className }: SkeletonProps) {
  return (
    <div
      className={[styles.skeleton, className].filter(Boolean).join(" ")}
      style={{ width, height }}
      aria-hidden="true"
    />
  );
}
