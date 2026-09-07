# Telegram Chat Stats

Локальное десктопное приложение (Tauri 2 + React/TypeScript) для анализа
экспортов переписки Telegram: статистика, дашборд и «слайды» с итогами
в духе годового обзора, которые можно сохранить как картинки. Работает
полностью офлайн — файл выгрузки не покидает компьютер, никакой сети в
рантайме и никакой телеметрии.

Инструмент универсальный: не привязан к конкретному размеру, периоду,
числу участников или языку чата, хранит сколько угодно импортов
одновременно и позволяет свободно переключаться между ними.

Полный технический спек — `docs/SPEC.md`, план разработки по фазам и
текущий статус — `docs/PLAN.md`, инструкции для работы над кодом —
`CLAUDE.md`.

## Стек

- Tauri 2 (Rust) — потоковый парсинг экспорта, SQLite (`rusqlite`),
  агрегация метрик
- React 19 + TypeScript + Vite — интерфейс, никаких вычислений метрик
  на фронте
- `@tanstack/react-table`, Radix UI — таблицы и диалоги
- recharts — графики

## Команды

```
pnpm install        # установка зависимостей
pnpm tauri dev       # запуск в разработке
pnpm tauri build     # сборка
pnpm build           # tsc --noEmit + vite build (проверка типов фронта)
cargo test           # тесты Rust (в src-tauri/)
cargo clippy         # линт Rust (в src-tauri/)
```

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
