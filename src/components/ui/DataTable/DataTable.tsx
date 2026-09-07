import {
  flexRender,
  getCoreRowModel,
  useReactTable,
  type ColumnDef,
  type OnChangeFn,
  type RowSelectionState,
} from "@tanstack/react-table";
import styles from "./DataTable.module.css";

// Тонкая обёртка над @tanstack/react-table (заголовок 3.4): сортировку,
// пагинацию, виртуализацию решает библиотека, здесь только вёрстка под
// токены. Выбор строк — опциональный тонкий прокидыш состояния в саму
// библиотеку (нужен шагу разбора участников, макет 3d), а не своя логика.
interface DataTableProps<T> {
  data: T[];
  columns: ColumnDef<T, any>[];
  className?: string;
  enableRowSelection?: boolean;
  rowSelection?: RowSelectionState;
  onRowSelectionChange?: OnChangeFn<RowSelectionState>;
  getRowId?: (row: T) => string;
}

export function DataTable<T>({
  data,
  columns,
  className,
  enableRowSelection,
  rowSelection,
  onRowSelectionChange,
  getRowId,
}: DataTableProps<T>) {
  const table = useReactTable({
    data,
    columns,
    getCoreRowModel: getCoreRowModel(),
    enableRowSelection,
    state: rowSelection ? { rowSelection } : undefined,
    onRowSelectionChange,
    getRowId,
  });

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
