import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { ChatSummary } from "../lib/types";

interface UseImportChatOptions {
  onImported: (chat: ChatSummary) => void;
}

// Диалог выбора файла + Tauri-команда импорта. Прогресс — без
// ingest://progress (см. docs/PLAN.md): маленький файл импортируется
// быстрее 400 мс и индикатор вообще не появляется, как требует SPEC §6.
export function useImportChat({ onImported }: UseImportChatOptions) {
  const [isImporting, setIsImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const importFile = useCallback(async () => {
    setError(null);
    const path = await open({
      multiple: false,
      filters: [{ name: "Telegram export", extensions: ["json"] }],
    });
    if (!path || Array.isArray(path)) return;

    const timer = setTimeout(() => setIsImporting(true), 400);
    try {
      const chat = await invoke<ChatSummary>("import_chat", { path });
      onImported(chat);
    } catch (e) {
      setError(String(e));
    } finally {
      clearTimeout(timer);
      setIsImporting(false);
    }
  }, [onImported]);

  return { importFile, isImporting, error };
}
