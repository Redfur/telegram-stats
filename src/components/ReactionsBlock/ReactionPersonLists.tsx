import { Card, CardKicker } from "../ui";
import type { ReactionPersonCount } from "../../lib/types";
import styles from "./ReactionsBlock.module.css";

interface ReactionPersonListsProps {
  given: ReactionPersonCount[];
  received: ReactionPersonCount[];
  givenDataIncomplete: boolean;
}

// "Кто ставит" / "Кому ставят" (docs/SPEC.md §9) — раздельные списки, не
// одна таблица: разная точность (см. пометку на "Кто ставит") и разный
// смысл счётчика, сравнивать их напрямую бессмысленно.
export function ReactionPersonLists({ given, received, givenDataIncomplete }: ReactionPersonListsProps) {
  if (given.length === 0 && received.length === 0) return null;
  return (
    <div className={styles.personGrid}>
      <PersonListCard
        title="Кто ставит"
        people={given}
        caveat={givenDataIncomplete ? "неполные данные — Telegram хранит не всех, кто отреагировал" : undefined}
      />
      <PersonListCard title="Кому ставят" people={received} />
    </div>
  );
}

function PersonListCard({
  title,
  people,
  caveat,
}: {
  title: string;
  people: ReactionPersonCount[];
  caveat?: string;
}) {
  const maxCount = Math.max(1, ...people.map((p) => p.count));
  return (
    <Card>
      <div className={styles.personHead}>
        <CardKicker>{title}</CardKicker>
        {caveat && <span className={styles.caveat}>{caveat}</span>}
      </div>
      {people.length === 0 ? (
        <div className={styles.empty}>Нет данных</div>
      ) : (
        <div className={styles.personList}>
          {people.map((p) => (
            <div key={p.person_id} className={styles.personRow}>
              <span className={styles.personName}>{p.display_name || "Без имени"}</span>
              <div className={styles.barTrack}>
                <div
                  className={styles.barFill}
                  style={{ width: `${Math.round((p.count / maxCount) * 100)}%` }}
                />
              </div>
              <span className="num">{p.count.toLocaleString("ru-RU")}</span>
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}
