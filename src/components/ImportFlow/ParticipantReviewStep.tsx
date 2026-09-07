import { useMemo, useState } from "react";
import type { ColumnDef, RowSelectionState } from "@tanstack/react-table";
import { Button, DataTable } from "../ui";
import type { PersonRow } from "../../lib/types";
import styles from "./ParticipantReviewStep.module.css";

const MONTH_YEAR = new Intl.DateTimeFormat("ru-RU", { month: "2-digit", year: "numeric" });

function formatActivity(first: number | null, last: number | null): string {
  if (first == null || last == null) return "—";
  const from = MONTH_YEAR.format(new Date(first * 1000));
  const to = MONTH_YEAR.format(new Date(last * 1000));
  return from === to ? from : `${from} — ${to}`;
}

interface ParticipantReviewStepProps {
  people: PersonRow[];
  onMerge: (primaryId: number, otherIds: number[]) => Promise<void>;
  onHide: (personId: number, hidden: boolean) => Promise<void>;
  onRename: (personId: number, displayName: string) => Promise<void>;
  onDone: () => void;
  onSkip: () => void;
}

// Макет «Экраны приложения» 3d, базовый объём (подтверждено с
// пользователем): переименование/скрытие/объединение дублей, без
// авто-детекта дублей/ботов, сегмент-фильтра и сворачивания хвоста —
// для них нет честного сигнала в данных на этом этапе (см. docs/PLAN.md).
export function ParticipantReviewStep({ people, onMerge, onHide, onRename, onDone, onSkip }: ParticipantReviewStepProps) {
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  const [renaming, setRenaming] = useState(false);
  const [nameDraft, setNameDraft] = useState("");

  const selected = useMemo(
    () => people.filter((p) => rowSelection[String(p.id)]),
    [people, rowSelection],
  );

  const columns: ColumnDef<PersonRow, any>[] = useMemo(
    () => [
      {
        id: "select",
        header: ({ table }) => (
          <input
            type="checkbox"
            checked={table.getIsAllRowsSelected()}
            onChange={table.getToggleAllRowsSelectedHandler()}
          />
        ),
        cell: ({ row }) => (
          <input type="checkbox" checked={row.getIsSelected()} onChange={row.getToggleSelectedHandler()} />
        ),
      },
      {
        header: "Участник",
        accessorKey: "display_name",
        cell: (info) => {
          const hidden = info.row.original.is_hidden;
          return (
            <span className={hidden ? styles.hiddenName : undefined}>
              {info.getValue<string | null>() || "Без имени"}
              {hidden && <span className={styles.hiddenTag}>скрыт</span>}
            </span>
          );
        },
      },
      {
        header: "Исторические имена",
        accessorKey: "aliases",
        cell: (info) => {
          const aliases = info.getValue<string[]>();
          return <span className={styles.aliases}>{aliases.length ? aliases.join(", ") : "—"}</span>;
        },
      },
      {
        header: "Сообщений",
        accessorKey: "message_count",
        cell: (info) => <span className="num">{info.getValue<number>().toLocaleString("ru-RU")}</span>,
      },
      {
        header: "Активность",
        id: "activity",
        cell: (info) => (
          <span className="num">{formatActivity(info.row.original.first_msg_ts, info.row.original.last_msg_ts)}</span>
        ),
      },
    ],
    [],
  );

  function startRename() {
    setNameDraft(selected[0]?.display_name ?? "");
    setRenaming(true);
  }

  async function confirmRename() {
    if (selected[0] && nameDraft.trim()) {
      await onRename(selected[0].id, nameDraft.trim());
    }
    setRenaming(false);
    setRowSelection({});
  }

  async function handleMerge() {
    const primary = selected.reduce((a, b) => (b.message_count > a.message_count ? b : a));
    const others = selected.filter((p) => p.id !== primary.id).map((p) => p.id);
    await onMerge(primary.id, others);
    setRowSelection({});
  }

  async function handleHideToggle() {
    const nextHidden = !selected.every((p) => p.is_hidden);
    await Promise.all(selected.map((p) => onHide(p.id, nextHidden)));
    setRowSelection({});
  }

  const allSelectedHidden = selected.length > 0 && selected.every((p) => p.is_hidden);

  return (
    <div className={styles.overlay}>
      <div className={styles.panel}>
        <div className={styles.headRow}>
          <div>
            <div className={["kick", styles.kicker].join(" ")}>ИМПОРТ · ШАГ 3 ИЗ 3</div>
            <h3 className={styles.title}>Участники</h3>
            <div className={styles.subtitle}>{people.length.toLocaleString("ru-RU")} найдено</div>
          </div>
          <Button variant="primary" onClick={onDone} className={styles.doneBtn}>
            Готово, к дашборду
          </Button>
        </div>

        {selected.length > 0 && (
          <div className={styles.actionBar}>
            {renaming ? (
              <>
                <input
                  className={styles.renameInput}
                  value={nameDraft}
                  onChange={(e) => setNameDraft(e.target.value)}
                  autoFocus
                />
                <Button variant="primary" onClick={confirmRename}>
                  Сохранить
                </Button>
                <Button variant="secondary" onClick={() => setRenaming(false)}>
                  Отмена
                </Button>
              </>
            ) : (
              <>
                <span className={styles.selectedCount}>Выбрано {selected.length}</span>
                <Button variant="primary" onClick={handleMerge} disabled={selected.length < 2}>
                  Объединить в одного
                </Button>
                <Button variant="secondary" onClick={startRename} disabled={selected.length !== 1}>
                  Переименовать
                </Button>
                <Button variant="secondary" onClick={handleHideToggle}>
                  {allSelectedHidden ? "Показать" : "Скрыть"}
                </Button>
              </>
            )}
          </div>
        )}

        <div className={styles.tableWrap}>
          <DataTable
            data={people}
            columns={columns}
            enableRowSelection
            rowSelection={rowSelection}
            onRowSelectionChange={setRowSelection}
            getRowId={(p) => String(p.id)}
          />
        </div>

        <div className={styles.footer}>
          <span className={styles.skip} onClick={onSkip}>
            Пропустить разбор
          </span>
        </div>
      </div>
    </div>
  );
}
