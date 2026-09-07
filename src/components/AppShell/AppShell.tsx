import type { ReactNode } from "react";
import styles from "./AppShell.module.css";

interface AppShellProps {
  rail: ReactNode;
  children: ReactNode;
}

// Каркас, общий для всех экранов приложения (см. docs/SPEC.md §10) — рельс
// слева + основная область. Ничего не знает про навигацию или чаты, это
// забота AppRail и конкретных экранов.
export function AppShell({ rail, children }: AppShellProps) {
  return (
    <div className={styles.shell}>
      <aside className={styles.rail}>{rail}</aside>
      <main className={styles.main}>{children}</main>
    </div>
  );
}
