import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PersonRow } from "../lib/types";

// Та же логика, что и в Dashboard.tsx/MessageTypesBlock.tsx — не бить
// invoke на каждый промежуточный chatId при быстром переключении чатов
// в рельсе (самостоятельный экран «Участники» держит тот же chatId-driven
// эффект, что и дашборд).
const SWITCH_DEBOUNCE_MS = 150;

// Данные и мутации шага «Разбор участников» (макет 3d). `chatId === null`
// — шаг закрыт, хук ничего не делает.
export function usePeopleReview(chatId: number | null) {
  const [people, setPeople] = useState<PersonRow[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (chatId == null) return;
    const result = await invoke<PersonRow[]>("get_people", { chatId });
    setPeople(result);
  }, [chatId]);

  useEffect(() => {
    if (chatId == null) {
      setPeople([]);
      setIsLoading(false);
      return;
    }
    let cancelled = false;
    setPeople([]);
    setIsLoading(true);
    const timer = setTimeout(() => {
      invoke<PersonRow[]>("get_people", { chatId }).then((result) => {
        if (cancelled) return;
        setPeople(result);
        setIsLoading(false);
      });
    }, SWITCH_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [chatId]);

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

  return { people, isLoading, refresh, merge, hide, rename };
}
