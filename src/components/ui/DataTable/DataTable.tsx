import { flexRender, getCoreRowModel, useReactTable, type ColumnDef } from "@tanstack/react-table";
import styles from "./DataTable.module.css";

// Тонкая обёртка над @tanstack/react-table (заголовок 3.4): сортировку,
// пагинацию, виртуализацию решает библиотека, здесь только вёрстка под токены.
interface DataTableProps<T> {
  data: T[];
  columns: ColumnDef<T, any>[];
  className?: string;
}

export function DataTable<T>({ data, columns, className }: DataTableProps<T>) {
  const table = useReactTable({ data, columns, getCoreRowModel: getCoreRowModel() });

  return (
    <table className={[styles.table, className].filter(Boolean).join(" ")}>
      <thead>
        {table.getHeaderGroups().map((headerGroup) => (
          <tr key={headerGroup.id}>
            {headerGroup.headers.map((header) => (
              <th key={header.id}>
                {header.isPlaceholder ? null : flexRender(header.column.columnDef.header, header.getContext())}
              </th>
            ))}
          </tr>
        ))}
      </thead>
      <tbody>
        {table.getRowModel().rows.map((row) => (
          <tr key={row.id}>
            {row.getVisibleCells().map((cell) => (
              <td key={cell.id}>{flexRender(cell.column.columnDef.cell, cell.getContext())}</td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
