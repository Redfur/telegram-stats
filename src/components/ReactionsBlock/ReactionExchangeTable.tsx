import type { ColumnDef } from "@tanstack/react-table";
import { CardKicker, DataTable } from "../ui";
import type { ReactionExchangeRow } from "../../lib/types";
import styles from "./ReactionsBlock.module.css";

interface ReactionExchangeTableProps {
  rows: ReactionExchangeRow[];
}

// «Обмен реакциями» (макет «Дашборд», ход t8/8a) — единая таблица вместо
// двух раздельных списков «кто ставит»/«кому ставят»: по каждому
// участнику сразу видно обе стороны, баланс между ними и какую реакцию
// он ставит чаще всего.
export function ReactionExchangeTable({ rows }: ReactionExchangeTableProps) {
  if (rows.length === 0) return null;

  const columns: ColumnDef<ReactionExchangeRow, any>[] = [
    {
      header: "Участник",
      accessorKey: "display_name",
      cell: (info) => info.row.original.display_name || "Без имени",
    },
    {
      header: "Ставит",
      accessorKey: "given_count",
      cell: (info) => <span className="num">{info.getValue<number>().toLocaleString("ru-RU")}</span>,
    },
    {
      header: "Получает",
      accessorKey: "received_count",
      cell: (info) => (
        <span className={["num", styles.accentCell].join(" ")}>
          {info.getValue<number>().toLocaleString("ru-RU")}
        </span>
      ),
    },
    {
      id: "balance",
      header: "Баланс",
      cell: (info) => {
        const row = info.row.original;
        const total = row.given_count + row.received_count;
        if (total === 0) return null;
        const giveShare = row.given_count / total;
        // Полоса растёт от центра (50%) в сторону перевеса — влево, если
        // ставит больше, чем получает, вправо — если наоборот.
        const left = giveShare <= 0.5 ? giveShare * 100 : 50;
        const width = Math.abs(giveShare - 0.5) * 100;
        const towardsGive = giveShare > 0.5;
        return (
          <div className={styles.balanceTrack}>
            <div className={styles.balanceAxis} />
            <div
              className={[styles.balanceFill, towardsGive ? styles.balanceGive : styles.balanceGet].join(
                " ",
              )}
              style={{ left: `${left}%`, width: `${width}%` }}
            />
          </div>
        );
      },
    },
    {
      id: "delta",
      header: "Δ",
      cell: (info) => {
        const row = info.row.original;
        const delta = row.received_count - row.given_count;
        const sign = delta > 0 ? "+" : "";
        return (
          <span
            className={["num", delta > 0 ? styles.accentCell : delta < 0 ? styles.mutedCell : ""]
              .filter(Boolean)
              .join(" ")}
          >
            {sign}
            {delta.toLocaleString("ru-RU")}
          </span>
        );
      },
    },
    {
      id: "favorite",
      header: "Чаще ставит",
      cell: (info) => {
        const favorite = info.row.original.favorite_given;
        if (!favorite) return <span className={styles.mutedCell}>—</span>;
        return <span className={styles.emojiGlyph}>{favorite.emoji ?? "🖼"}</span>;
      },
    },
  ];

  return (
    <div>
      <CardKicker>Обмен реакциями</CardKicker>
      <DataTable data={rows} columns={columns} getRowId={(r) => String(r.person_id)} />
    </div>
  );
}
