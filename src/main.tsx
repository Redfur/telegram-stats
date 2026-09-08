import React from "react";
import ReactDOM from "react-dom/client";
import { MemoryRouter } from "react-router-dom";
import App from "./App";
import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/base.css";

// MemoryRouter — окно приложения не браузер, адресной строки и
// диплинков нет, история живёт только в памяти процесса.
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <MemoryRouter>
      <App />
    </MemoryRouter>
  </React.StrictMode>,
);
