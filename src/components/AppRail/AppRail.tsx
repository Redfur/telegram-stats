import styles from "./AppRail.module.css";

export type NavTarget = "library" | "people" | "import" | "slides";

const NAV_ITEMS: { target: NavTarget; label: string }[] = [
  { target: "library", label: "Библиотека" },
  { target: "people", label: "Участники" },
  { target: "import", label: "Импорт" },
  { target: "slides", label: "Слайды" },
];

interface AppRailProps {
  activeNav: NavTarget;
  onNavigate: (target: NavTarget) => void;
}

// Список чатов пока не показываем — источника данных нет до Фазы 1/2.
// Проп `chats` появится вместе с реальным импортом, а не раньше.
export function AppRail({ activeNav, onNavigate }: AppRailProps) {
  return (
    <div className={styles.rail}>
      <div className={styles.brand}>TG&nbsp;/&nbsp;ARCHIVE</div>
      <div className={styles.chatsHeader}>
        <span className="kick">ЧАТЫ</span>
      </div>
      <div className={styles.empty}>Пока ничего не импортировано</div>
      <nav className={styles.nav}>
        {NAV_ITEMS.map((item) => (
          <button
            key={item.target}
            type="button"
            className={[styles.navItem, item.target === activeNav ? styles.active : ""].filter(Boolean).join(" ")}
            aria-current={item.target === activeNav ? "page" : undefined}
            onClick={() => onNavigate(item.target)}
          >
            {item.label}
          </button>
        ))}
      </nav>
    </div>
  );
}
