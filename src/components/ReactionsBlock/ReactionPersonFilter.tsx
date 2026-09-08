import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, CardKicker, Skeleton } from "../ui";
import type { PersonReactionBreakdown, PersonRow, ReactionEmojiCount } from "../../lib/types";
import styles from "./ReactionsBlock.module.css";

interface ReactionPersonFilterProps {
  chatId: number;
  people: PersonRow[];
  onReimport: () => void;
}

// Фильтр по участнику (запрошено отдельно от общих топ-10 given/received):
// в отличие от них, выбор не ограничен видимой десяткой — селект несёт
// всех участников чата (переиспользует get_people, тот же список, что и
// экран «Участники»), а разбивку по конкретным эмодзи тянет отдельным
// invoke только по выбору, не для всех сразу.
export function ReactionPersonFilter({ chatId, people, onReimport }: ReactionPersonFilterProps) {
  const [personId, setPersonId] = useState<number | null>(null);
  const [breakdown, setBreakdown] = useState<PersonReactionBreakdown | null>(null);

  useEffect(() => {
    if (personId == null) {
      setBreakdown(null);
      return;
    }
    let cancelled = false;
    setBreakdown(null);
    invoke<PersonReactionBreakdown>("get_reaction_person_breakdown", { chatId, personId }).then((r) => {
      if (!cancelled) setBreakdown(r);
    });
    return () => {
      cancelled = true;
    };
  }, [chatId, personId]);

  const sorted = [...people].sort((a, b) =>
    (a.display_name || "").localeCompare(b.display_name || "", "ru"),
  );

  return (
    <Card>
      <CardKicker>Разбивка по участнику</CardKicker>
      <select
        className={styles.personPicker}
        value={personId ?? ""}
        onChange={(e) => setPersonId(e.target.value ? Number(e.target.value) : null)}
      >
        <option value="">Выберите участника…</option>
        {sorted.map((p) => (
          <option key={p.id} value={p.id}>
            {p.display_name || "Без имени"}
          </option>
        ))}
      </select>

      {personId != null && breakdown == null && (
        <div className={styles.breakdownGrid} aria-busy="true">
          <Skeleton height={60} />
          <Skeleton height={60} />
        </div>
      )}

      {breakdown && (
        <div className={styles.breakdownGrid}>
          <EmojiMiniList
            title="Ставит"
            items={breakdown.given}
            total={breakdown.given_total}
            onReimport={onReimport}
          />
          <EmojiMiniList
            title="Получает"
            items={breakdown.received}
            total={breakdown.received_total}
            onReimport={onReimport}
          />
        </div>
      )}
    </Card>
  );
}

function EmojiMiniList({
  title,
  items,
  total,
  onReimport,
}: {
  title: string;
  items: ReactionEmojiCount[];
  total: number;
  onReimport: () => void;
}) {
  if (total === 0) {
    return (
      <div>
        <div className={styles.miniListTitle}>{title}</div>
        <div className={styles.empty}>Нет данных</div>
      </div>
    );
  }
  // total > 0, но разбивки нет — реакции посчитаны до появления разбивки
  // по эмодзи (миграция 0009), общий счётчик при этом уже верный.
  if (items.length === 0) {
    return (
      <div>
        <div className={styles.miniListTitle}>{title}</div>
        <p className={styles.caveat}>
          Разбивка по эмодзи недоступна для части реакций —{" "}
          <button type="button" className={styles.inlineAction} onClick={onReimport}>
            переимпортируйте чат
          </button>
        </p>
      </div>
    );
  }

  const maxCount = Math.max(1, ...items.map((i) => i.count));
  return (
    <div>
      <div className={styles.miniListTitle}>{title}</div>
      <div className={styles.emojiList}>
        {items.map((e, i) => (
          <div key={`${e.kind}-${e.emoji ?? e.document_id ?? i}`} className={styles.emojiRow}>
            <span className={styles.emojiGlyph}>{e.emoji ?? "🖼"}</span>
            <div className={styles.barTrack}>
              <div
                className={styles.barFill}
                style={{ width: `${Math.round((e.count / maxCount) * 100)}%` }}
              />
            </div>
            <span className="num">{e.count.toLocaleString("ru-RU")}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
