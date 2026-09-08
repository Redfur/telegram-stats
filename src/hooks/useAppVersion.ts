import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";

// Версия из бандла (tauri.conf.json на момент сборки), не сетевой запрос.
export function useAppVersion() {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setVersion);
  }, []);

  return version;
}
