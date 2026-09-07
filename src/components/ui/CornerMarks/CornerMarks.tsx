import styles from "./CornerMarks.module.css";

// Регистрационные метки по углам обязательны у любого обрамлённого объекта
// (см. макет «Дизайн-токены» §5b). Родитель должен быть position: relative.
export function CornerMarks() {
  return (
    <>
      <i className={[styles.corner, styles.tl].join(" ")} />
      <i className={[styles.corner, styles.tr].join(" ")} />
      <i className={[styles.corner, styles.bl].join(" ")} />
      <i className={[styles.corner, styles.br].join(" ")} />
    </>
  );
}
