import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ChatSummary } from "../lib/types";

export function useChats() {
  const [chats, setChats] = useState<ChatSummary[]>([]);

  const refresh = useCallback(async () => {
    const result = await invoke<ChatSummary[]>("list_chats");
    setChats(result);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { chats, refresh };
}
