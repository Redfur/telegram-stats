import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PersonRow } from "../lib/types";

// Данные и мутации шага «Разбор участников» (макет 3d). `chatId === null`
// — шаг закрыт, хук ничего не делает.
export function usePeopleReview(chatId: number | null) {
  const [people, setPeople] = useState<PersonRow[]>([]);

  const refresh = useCallback(async () => {
    if (chatId == null) return;
    const result = await invoke<PersonRow[]>("get_people", { chatId });
    setPeople(result);
  }, [chatId]);

  useEffect(() => {
    if (chatId != null) {
      void refresh();
    } else {
      setPeople([]);
    }
  }, [chatId, refresh]);

  const merge = useCallback(
    async (primaryId: number, otherIds: number[]) => {
      if (chatId == null) return;
      await invoke("merge_people", { chatId, primaryId, otherIds });
      await refresh();
    },
    [chatId, refresh],
  );

  const hide = useCallback(
    async (personId: number, hidden: boolean) => {
      if (chatId == null) return;
      await invoke("hide_person", { chatId, personId, hidden });
      await refresh();
    },
    [chatId, refresh],
  );

  const rename = useCallback(
    async (personId: number, displayName: string) => {
      if (chatId == null) return;
      await invoke("rename_person", { chatId, personId, displayName });
      await refresh();
    },
    [chatId, refresh],
  );

  return { people, refresh, merge, hide, rename };
}
