import styles from "./ComingSoon.module.css";

interface ComingSoonProps {
  title: string;
  message?: string;
}

export function ComingSoon({ title, message = "Экран появится в следующей фазе" }: ComingSoonProps) {
  return (
    <div className={styles.wrap}>
      <span className="kick">{title}</span>
      <h2 className={styles.heading}>{message}</h2>
    </div>
  );
}
