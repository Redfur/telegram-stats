import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Command } from "cmdk";
import { CardKicker, Skeleton } from "../ui";
import type { PersonReactionBreakdown, PersonRow, ReactionEmojiCount } from "../../lib/types";
import styles from "./ReactionsBlock.module.css";

interface ReactionPersonFilterProps {
  chatId: number;
  people: PersonRow[];
  onReimport: () => void;
}

interface DivergingRow {
  emoji: string | null;
  give: number;
  get: number;
}

// Разбивка может быть длинной (десятки разных эмодзи, часть — с одним
// использованием) — по умолчанию видно топ-10, остальное под «показать
// ещё», тот же приём, что и у сетки эмодзи в сводке.
const VISIBLE_DIVERGING_COUNT = 10;

function mergeBreakdown(given: ReactionEmojiCount[], received: ReactionEmojiCount[]): DivergingRow[] {
  const key = (e: ReactionEmojiCount) => `${e.kind}|${e.emoji ?? ""}|${e.document_id ?? ""}`;
  const rows = new Map<string, DivergingRow>();
  for (const e of given) {
    rows.set(key(e), { emoji: e.emoji, give: e.count, get: 0 });
  }
  for (const e of received) {
    const k = key(e);
    const existing = rows.get(k);
    if (existing) existing.get = e.count;
    else rows.set(k, { emoji: e.emoji, give: 0, get: e.count });
  }
  return [...rows.values()].sort((a, b) => b.give + b.get - (a.give + a.get));
}

// Подсвечивает первое вхождение query в name (без учёта регистра) — тот
// же приём, что показан в макете 8b для поиска участника.
function highlightMatch(name: string, query: string) {
  if (!query) return name;
  const idx = name.toLowerCase().indexOf(query.toLowerCase());
  if (idx === -1) return name;
  return (
    <>
      {name.slice(0, idx)}
      <span className={styles.matchHighlight}>{name.slice(idx, idx + query.length)}</span>
      {name.slice(idx + query.length)}
    </>
  );
}

// Кастомный поисковый селект участника (макет «Дашборд», ход t8/8b) —
// cmdk даёт фильтрацию/клавиатурную навигацию/a11y, подсветку совпадений
// и позиционирование делаем сами (свою тему на headless-базу).
export function ReactionPersonFilter({ chatId, people, onReimport }: ReactionPersonFilterProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [personId, setPersonId] = useState<number | null>(null);
  const [breakdown, setBreakdown] = useState<PersonReactionBreakdown | null>(null);
  const [expanded, setExpanded] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  function reset() {
    setPersonId(null);
    setBreakdown(null);
    setExpanded(false);
    setOpen(false);
    setSearch("");
  }

  useEffect(() => {
    if (!open) return;
    function onPointerDown(e: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", onPointerDown);
    return () => document.removeEventListener("mousedown", onPointerDown);
  }, [open]);

  useEffect(() => {
    if (personId == null) {
      setBreakdown(null);
      return;
    }
    let cancelled = false;
    setBreakdown(null);
    setExpanded(false);
    invoke<PersonReactionBreakdown>("get_reaction_person_breakdown", { chatId, personId }).then((r) => {
      if (!cancelled) setBreakdown(r);
    });
    return () => {
      cancelled = true;
    };
  }, [chatId, personId]);

  const sorted = useMemo(
    () => [...people].sort((a, b) => (a.display_name || "").localeCompare(b.display_name || "", "ru")),
    [people],
  );
  const selected = people.find((p) => p.id === personId) ?? null;

  const allDiverging = breakdown ? mergeBreakdown(breakdown.given, breakdown.received) : [];
  const diverging = expanded ? allDiverging : allDiverging.slice(0, VISIBLE_DIVERGING_COUNT);
  const maxGive = Math.max(1, ...diverging.map((r) => r.give));
  const maxGet = Math.max(1, ...diverging.map((r) => r.get));

  return (
    <div>
      <div className={styles.personFilterHead}>
        <CardKicker>Разбивка по участнику</CardKicker>
        <div className={styles.personPickerWrap} ref={rootRef}>
          <button
            type="button"
            className={styles.personPickerTrigger}
            onClick={() => setOpen((v) => !v)}
          >
            <span className={styles.personPickerLabel}>
              {selected ? selected.display_name || "Без имени" : "Выберите участника"}
            </span>
            <span className={styles.personPickerCaret}>{open ? "▲" : "▼"}</span>
          </button>
          {selected && (
            <button
              type="button"
              className={styles.personPickerClear}
              onClick={reset}
              title="Сбросить фильтр"
              aria-label="Сбросить фильтр"
            >
              ✕
            </button>
          )}
          {open && (
            <Command className={styles.personPickerPanel} shouldFilter={false} label="Поиск участника">
              <Command.Input
                autoFocus
                value={search}
                onValueChange={setSearch}
                placeholder="поиск участника"
                className={styles.personPickerSearch}
              />
              <Command.List className={styles.personPickerList}>
                <Command.Empty className={styles.personPickerEmpty}>
                  Никого не нашли — проверьте написание или объедините дубли в разборе участников.
                </Command.Empty>
                {sorted
                  .filter((p) =>
                    (p.display_name || "").toLowerCase().includes(search.trim().toLowerCase()),
                  )
                  .map((p) => (
                    <Command.Item
                      key={p.id}
                      value={String(p.id)}
                      onSelect={() => {
                        setPersonId(p.id);
                        setOpen(false);
                        setSearch("");
                      }}
                      className={styles.personPickerItem}
                    >
                      <span className={styles.personPickerMark}>{p.id === personId ? "✓" : ""}</span>
                      <span className={styles.personPickerItemName}>
                        {highlightMatch(p.display_name || "Без имени", search.trim())}
                      </span>
                      <span className={styles.personPickerItemCount}>
                        {p.message_count.toLocaleString("ru-RU")}
                      </span>
                    </Command.Item>
                  ))}
              </Command.List>
            </Command>
          )}
        </div>
      </div>

      {personId != null && breakdown == null && (
        <div className={styles.breakdownGrid} aria-busy="true">
          <Skeleton height={80} />
        </div>
      )}

      {breakdown && breakdown.given_total === 0 && breakdown.received_total === 0 && (
        <div className={styles.empty}>
          Ни поставленных, ни полученных реакций — участник вышел до того, как они появились в чате.
        </div>
      )}

      {breakdown && (breakdown.given_total > 0 || breakdown.received_total > 0) && (
        <div className={styles.divergingWrap}>
          <div className={styles.divergingTotals}>
            <span className={styles.divergingTotalGive}>
              СТАВИТ <span className="num">{breakdown.given_total.toLocaleString("ru-RU")}</span>
            </span>
            <span className={styles.divergingTotalGet}>
              <span className="num">{breakdown.received_total.toLocaleString("ru-RU")}</span> ПОЛУЧАЕТ
            </span>
          </div>
          {breakdown.given_total > 0 && breakdown.given.length === 0 && (
            <p className={styles.caveat}>
              Разбивка по эмодзи для части реакций недоступна —{" "}
              <button type="button" className={styles.inlineAction} onClick={onReimport}>
                переимпортируйте чат
              </button>
            </p>
          )}
          {diverging.map((row, i) => (
            <div key={`${row.emoji ?? i}`} className={styles.divergingRow}>
              <div className={styles.divergingBarGive}>
                <div
                  className={styles.divergingFillGive}
                  style={{ width: `${Math.round((row.give / maxGive) * 100)}%` }}
                />
              </div>
              <span className="num">{row.give.toLocaleString("ru-RU")}</span>
              <span className={styles.emojiGlyph}>{row.emoji ?? "🖼"}</span>
              <span className="num">{row.get.toLocaleString("ru-RU")}</span>
              <div className={styles.divergingBarGet}>
                <div
                  className={styles.divergingFillGet}
                  style={{ width: `${Math.round((row.get / maxGet) * 100)}%` }}
                />
              </div>
            </div>
          ))}
          {allDiverging.length > VISIBLE_DIVERGING_COUNT && (
            <button type="button" className={styles.showAll} onClick={() => setExpanded((v) => !v)}>
              {expanded
                ? "свернуть"
                : `показать все (ещё ${allDiverging.length - VISIBLE_DIVERGING_COUNT}) ▾`}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
