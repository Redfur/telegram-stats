import { useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Button, Card, CardKicker, DataTable, Segmented, Tag } from "./components/ui";

// Временная витрина примитивов — проверка токенов, шрифтов и компонентов
// перед тем, как поверх них строить настоящие экраны (см. docs/SPEC.md §10).
// Заменяется первым реальным экраном (Библиотека).

interface Row {
  name: string;
  messages: number;
  tag: string;
}

const rows: Row[] = [
  { name: "Паша", messages: 231004, tag: "группа" },
  { name: "Deleted Account", messages: 20814, tag: "бот" },
];

const columns: ColumnDef<Row>[] = [
  { header: "Отображаемое имя", accessorKey: "name" },
  {
    header: "Сообщений",
    accessorKey: "messages",
    cell: (info) => <span className="num">{info.getValue<number>().toLocaleString("ru-RU")}</span>,
  },
  {
    header: "Метка",
    accessorKey: "tag",
    cell: (info) => <Tag variant={info.getValue() === "бот" ? "outline" : "accent"}>{info.getValue<string>()}</Tag>,
  },
];

function App() {
  const [period, setPeriod] = useState<"all" | "year" | "range">("all");

  return (
    <main style={{ padding: "34px 40px", display: "flex", flexDirection: "column", gap: "26px", maxWidth: 640 }}>
      <div>
        <span className="kick">Витрина примитивов</span>
        <h1 style={{ fontSize: 28, marginTop: 8 }}>Стареем но не сдаёмся</h1>
        <p style={{ color: "var(--text-muted)", marginTop: 4 }}>
          847 213 сообщения · март 2015 — декабрь 2025 · Паша, Марина, Дэн
        </p>
      </div>

      <section style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <Button variant="primary">Открыть</Button>
        <Button variant="secondary">Обновить выгрузкой</Button>
        <Button variant="ghost">Удалить</Button>
        <Button variant="primary" disabled>
          Недоступно
        </Button>
      </section>

      <Card>
        <CardKicker>Рекорд дня</CardKicker>
        <div className="num" style={{ font: "600 46px/.95 var(--font-head)" }}>
          4 312
        </div>
        <div style={{ fontSize: 12, color: "var(--text-muted)" }}>14 июня 2018 · ×9 к медиане</div>
      </Card>

      <Segmented
        options={[
          { value: "all", label: "Всё время" },
          { value: "year", label: "Год" },
          { value: "range", label: "Интервал…" },
        ]}
        value={period}
        onChange={setPeriod}
      />

      <DataTable data={rows} columns={columns} />
    </main>
  );
}

export default App;
