import { useState } from "react";
import { Button } from "../ui";
import type { MessageTypeCount, MessageTypeKind } from "../../lib/types";
import { MESSAGE_TYPE_LABELS } from "../../lib/messageTypeLabels";
import styles from "./MessageTypeToggle.module.css";

interface MessageTypeToggleProps {
  counts: MessageTypeCount[];
  selected: MessageTypeKind | null;
  onSelect: (kind: MessageTypeKind) => void;
}

// Топ-N по счётчику + разворачивание остатка по клику — как в макете
// (6a показывает 7 из 11, 6b — 4 из 11 с «ещё 7 ▾»), без пиннинга
// выбранного типа: сортировка по count уже держит его в видимой части
// в обеих демонстрируемых раскладках.
const VISIBLE_COUNT = 7;

export function MessageTypeToggle({ counts, selected, onSelect }: MessageTypeToggleProps) {
  const [expanded, setExpanded] = useState(false);
  const sorted = [...counts].sort((a, b) => b.count - a.count);
  const visible = expanded ? sorted : sorted.slice(0, VISIBLE_COUNT);
  const hiddenCount = sorted.length - visible.length;

  return (
    <div className={styles.row}>
      {visible.map((c) => (
        <Button
          key={c.kind}
          type="button"
          variant={c.kind === selected ? "primary" : "secondary"}
          disabled={c.count === 0}
          className={styles.pill}
          onClick={() => onSelect(c.kind)}
        >
          {MESSAGE_TYPE_LABELS[c.kind].plural}{" "}
          <span className={styles.count}>{c.count.toLocaleString("ru-RU")}</span>
        </Button>
      ))}
      {hiddenCount > 0 && (
        <Button type="button" variant="ghost" className={styles.pill} onClick={() => setExpanded(true)}>
          ещё {hiddenCount} ▾
        </Button>
      )}
    </div>
  );
}
