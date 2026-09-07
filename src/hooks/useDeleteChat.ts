import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export function useDeleteChat() {
  const [isDeleting, setIsDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const deleteChat = useCallback(async (chatId: number) => {
    setError(null);
    setIsDeleting(true);
    try {
      await invoke("delete_chat", { chatId });
    } catch (e) {
      setError(String(e));
      throw e;
    } finally {
      setIsDeleting(false);
    }
  }, []);

  return { deleteChat, isDeleting, error };
}
