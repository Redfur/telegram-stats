import type { ColumnDef } from "@tanstack/react-table";
import { DataTable } from "../ui";
import type { MessageTypeParticipantStat } from "../../lib/types";
import styles from "./MessageTypeParticipantsTable.module.css";

interface MessageTypeParticipantsTableProps {
  participants: MessageTypeParticipantStat[];
  typeColumnHeader: string;
}

// Четвёртая колонка зависит от того, какой вариант принесла метрика
// (макет 6a/6b): «На 1000» для частотных типов, «Всего минут» для
// голосовых/кружочков — сам массив участников уже несёт нужный вариант
// в `extra`, отдельного пропа не нужно.
export function MessageTypeParticipantsTable({
  participants,
  typeColumnHeader,
}: MessageTypeParticipantsTableProps) {
  const maxCount = Math.max(1, ...participants.map((p) => p.count));
  const extraHeader = participants.some((p) => p.extra.kind === "total_minutes") ? "Всего минут" : "На 1000";

  const columns: ColumnDef<MessageTypeParticipantStat, any>[] = [
    {
      header: "Участник",
      accessorKey: "display_name",
      cell: (info) => {
        const row = info.row.original;
        return (
          <span className={row.count === 0 ? styles.muted : undefined}>{row.display_name || "Без имени"}</span>
        );
      },
    },
    {
      header: `${typeColumnHeader} ↓`,
      accessorKey: "count",
      cell: (info) => {
        const value = info.getValue<number>();
        return (
          <span className={["num", value === 0 ? styles.muted : ""].filter(Boolean).join(" ")}>
            {value.toLocaleString("ru-RU")}
          </span>
        );
      },
    },
    {
      header: "Доля своих",
      accessorKey: "share_of_own_pct",
      cell: (info) => {
        const row = info.row.original;
        if (row.count === 0) return <span className={styles.muted}>—</span>;
        return <span className="num">{info.getValue<number>().toFixed(1).replace(".", ",")}%</span>;
      },
    },
    {
      id: "dynamics",
      header: "Динамика",
      cell: (info) => {
        const row = info.row.original;
        const width = row.count === 0 ? 0 : Math.round((row.count / maxCount) * 100);
        return (
          <div className={styles.barTrack}>
            <div className={styles.barFill} style={{ width: `${width}%` }} />
          </div>
        );
      },
    },
    {
      id: "extra",
      header: extraHeader,
      cell: (info) => {
        const row = info.row.original;
        if (row.count === 0) return <span className={styles.muted}>—</span>;
        return <span className="num">{Math.round(row.extra.value).toLocaleString("ru-RU")}</span>;
      },
    },
  ];

  return <DataTable data={participants} columns={columns} getRowId={(p) => String(p.person_id)} />;
}
