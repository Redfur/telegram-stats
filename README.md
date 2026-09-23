# Telegram Chat Stats

Локальное десктопное приложение (Tauri 2 + React/TypeScript) для анализа экспортов переписки Telegram: статистика, дашборд и «слайды» с итогами в духе годового обзора, которые можно сохранить как картинки. Работает полностью офлайн — файл выгрузки не покидает компьютер, никакой сети в рантайме и никакой телеметрии.

Инструмент универсальный: не привязан к конкретному размеру, периоду, числу участников или языку чата, хранит сколько угодно импортов одновременно и позволяет свободно переключаться между ними.

Полный технический спек — `docs/SPEC.md`, план разработки по фазам и текущий статус — `docs/PLAN.md`, инструкции для работы над кодом — `CLAUDE.md`.

## Стек

- Tauri 2 (Rust) — потоковый парсинг экспорта, SQLite (`rusqlite`), агрегация метрик
- React 19 + TypeScript + Vite — интерфейс, никаких вычислений метрик на фронте
- `@tanstack/react-table`, Radix UI — таблицы и диалоги
- recharts — графики

## Окружение разработки

Нужны Node 22+, pnpm (в CI — 12) и Rust stable через [rustup](https://rustup.rs/). Всё остальное зависит от платформы.

### Windows (целевая платформа, здесь собирается инсталлятор)

1. [Node LTS](https://nodejs.org/) → `corepack enable pnpm`
2. [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) — рабочая нагрузка «Разработка классических приложений на C++» (нужен MSVC-линкер для Rust и для `rusqlite` с `bundled`)
3. [rustup](https://rustup.rs/) — тулчейн `stable-x86_64-pc-windows-msvc`
4. [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) — в Windows 11 уже установлен

### WSL2 (Ubuntu 24.04) — разработка и тесты

GUI требует WSLg (Windows 11 или обновлённый WSL 2 на Windows 10). Проверка: `echo $DISPLAY` должен вернуть `:0`.

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config curl wget file \
  libwebkit2gtk-4.1-dev libxdo-dev libssl-dev \
  libayatana-appindicator3-dev librsvg2-dev

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

pnpm install
```

Node и pnpm ставятся отдельно — [nvm](https://github.com/nvm-sh/nvm) или пакетом дистрибутива, затем `corepack enable pnpm`. Не используйте Node/pnpm, установленные в Windows: запуск через `/mnt/c` ломает нативные модули и работает крайне медленно.

Две особенности WSL2:

- `pnpm tauri build` здесь не даст инсталлятор: в `bundle.targets` только `nsis`, это Windows-формат. Под WSL собирается и запускается только dev-версия (`pnpm tauri dev`); шипуемый бинарник — нативно на Windows либо через workflow `build.yml` / `release.yml`.
- Если окно приложения открывается белым или падает на старте, у WebKitGTK конфликт с программным рендерингом WSLg. Лечится переменными окружения:

  ```bash
  export WEBKIT_DISABLE_DMABUF_RENDERER=1
  export WEBKIT_DISABLE_COMPOSITING_MODE=1
  ```

### Проверка

```bash
pnpm build                    # типы + сборка фронта
cd src-tauri && cargo test    # тесты Rust
pnpm tauri dev                # запуск приложения
```

Первая сборка Rust идёт долго: `rusqlite` собирается с `bundled`-SQLite из исходников.

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
