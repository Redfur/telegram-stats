import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { ChatSummary } from "../lib/types";

export type ImportPhase = "reading" | "finalizing";

export interface ImportProgress {
  phase: ImportPhase;
  bytes_read: number;
  total_bytes: number | null;
  message_count: number;
  people_count: number;
}

export type ImportErrorKind = "not_telegram_export" | "invalid_json" | "message_error" | "cancelled" | "io";

export interface ImportErrorInfo {
  kind: ImportErrorKind;
  message: string;
}

function parseImportError(e: unknown): ImportErrorInfo {
  const raw = String(e);
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed.kind === "string" && typeof parsed.message === "string") {
      return parsed as ImportErrorInfo;
    }
  } catch {
    // ошибка не в нашем JSON-формате (например, паника JoinError) — покажем как есть
  }
  return { kind: "io", message: raw };
}

interface UseImportChatOptions {
  onImported: (chat: ChatSummary) => void;
}

// Диалог выбора файла + Tauri-команда импорта, с настоящим прогрессом
// (`ingest://progress`, см. docs/PLAN.md) вместо таймера-имитации:
// если ни одного события не пришло до резолва `invoke`, импорт был
// мгновенным и оверлей прогресса просто не успевает появиться — то же
// требование SPEC §6 «маленький файл — без модалок», без искусственного
// таймера.
export function useImportChat({ onImported }: UseImportChatOptions) {
  const [isBusy, setIsBusy] = useState(false);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<ImportErrorInfo | null>(null);

  const importFile = useCallback(async () => {
    setError(null);
    const path = await open({
      multiple: false,
      filters: [{ name: "Telegram export", extensions: ["json"] }],
    });
    if (!path || Array.isArray(path)) return;

    setIsBusy(true);
    setProgress(null);
    const unlisten = await listen<ImportProgress>("ingest://progress", (event) => {
      setProgress(event.payload);
    });
    try {
      const chat = await invoke<ChatSummary>("import_chat", { path });
      onImported(chat);
    } catch (e) {
      setError(parseImportError(e));
    } finally {
      unlisten();
      setIsBusy(false);
      setProgress(null);
    }
  }, [onImported]);

  const cancelImport = useCallback(() => {
    void invoke("cancel_import");
  }, []);

  const dismissError = useCallback(() => setError(null), []);

  return {
    importFile,
    cancelImport,
    dismissError,
    isImporting: isBusy,
    progress,
    error,
  };
}
