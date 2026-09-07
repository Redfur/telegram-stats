import styles from "./ComingSoon.module.css";

interface ComingSoonProps {
  title: string;
}

export function ComingSoon({ title }: ComingSoonProps) {
  return (
    <div className={styles.wrap}>
      <span className="kick">{title}</span>
      <h2 className={styles.heading}>Экран появится в следующей фазе</h2>
    </div>
  );
}
