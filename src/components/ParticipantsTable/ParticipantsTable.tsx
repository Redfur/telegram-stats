import type { ColumnDef } from "@tanstack/react-table";
import { DataTable } from "../ui";
import type { ParticipantStat } from "../../lib/types";

interface ParticipantsTableProps {
  participants: ParticipantStat[];
}

const columns: ColumnDef<ParticipantStat, any>[] = [
  {
    header: "Участник",
    accessorKey: "display_name",
    cell: (info) => info.getValue<string | null>() || "Без имени",
  },
  {
    header: "Сообщений",
    accessorKey: "message_count",
    cell: (info) => <span className="num">{info.getValue<number>().toLocaleString("ru-RU")}</span>,
  },
  {
    header: "Средняя длина",
    accessorKey: "avg_len",
    cell: (info) => <span className="num">{Math.round(info.getValue<number>())}</span>,
  },
];

export function ParticipantsTable({ participants }: ParticipantsTableProps) {
  return <DataTable data={participants} columns={columns} />;
}
