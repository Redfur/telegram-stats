// Временная витрина примитивов — проверка токенов, шрифтов и компонентов
// перед тем, как поверх них строить настоящие экраны (см. docs/SPEC.md §10).
// Заменяется первым реальным экраном (Библиотека).
function App() {
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
        <button className="btn btn-primary">Открыть</button>
        <button className="btn btn-secondary">Обновить выгрузкой</button>
        <button className="btn btn-ghost">Удалить</button>
        <button className="btn btn-primary" disabled>
          Недоступно
        </button>
      </section>

      <section className="card">
        <i className="corner tl" />
        <i className="corner tr" />
        <i className="corner bl" />
        <i className="corner br" />
        <span className="card-kicker">Рекорд дня</span>
        <div className="num" style={{ font: "600 46px/.95 var(--font-head)" }}>
          4 312
        </div>
        <div style={{ fontSize: 12, color: "var(--text-muted)" }}>14 июня 2018 · ×9 к медиане</div>
      </section>

      <section className="seg" style={{ width: "fit-content" }}>
        <span className="seg-opt on">Всё время</span>
        <span className="seg-opt">Год</span>
        <span className="seg-opt">Интервал…</span>
      </section>

      <table className="table">
        <thead>
          <tr>
            <th>Отображаемое имя</th>
            <th style={{ textAlign: "right" }}>Сообщений</th>
            <th>Метка</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Паша</td>
            <td className="num" style={{ textAlign: "right" }}>
              231 004
            </td>
            <td>
              <span className="tag tag-accent">группа</span>
            </td>
          </tr>
          <tr>
            <td>Deleted Account</td>
            <td className="num" style={{ textAlign: "right", color: "var(--text-faint)" }}>
              20 814
            </td>
            <td>
              <span className="tag tag-outline">бот</span>
            </td>
          </tr>
        </tbody>
      </table>
    </main>
  );
}

export default App;
