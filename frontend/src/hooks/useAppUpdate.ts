import { useState, useCallback } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { invoke } from "@tauri-apps/api/core";

export type UpdateState = "idle" | "checking" | "available" | "upToDate" | "downloading" | "installed" | "error";

export function useAppUpdate() {
  const [state, setState] = useState<UpdateState>("idle");
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState<number>(0);
  const [error, setError] = useState<string | null>(null);

  const checkForUpdates = useCallback(async () => {
    setState("checking");
    setError(null);
    setProgress(0);

    const timeout = new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error("업데이트 확인 시간 초과 (10초)")), 10000)
    );

    try {
      const updateResult = await Promise.race([check(), timeout]);
      if (updateResult) {
        setUpdate(updateResult);
        setState("available");
        return updateResult;
      } else {
        setUpdate(null);
        setState("upToDate");
        return null;
      }
    } catch (err: any) {
      console.error("[Updater] Check failed:", err);
      setState("error");
      setError(err.message || String(err));
      return null;
    }
  }, []);

  const installUpdate = useCallback(async () => {
    if (!update) return;

    setState("downloading");
    setProgress(0);
    setError(null);

    try {
      let contentLength: number | null = null;
      let downloaded = 0;

      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          contentLength = event.data.contentLength ?? null;
          downloaded = 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          const pct =
            contentLength && contentLength > 0
              ? Math.min(Math.round((downloaded / contentLength) * 100), 99)
              : 0;
          setProgress(pct);
        } else if (event.event === "Finished") {
          setProgress(100);
          setState("installed");
        }
      });
    } catch (err: any) {
      console.error("[Updater] Download/Install failed:", err);
      setState("error");
      setError(err.message || String(err));
    }
  }, [update]);

  const relaunchApp = useCallback(async () => {
    try {
      await invoke("relaunch_app");
    } catch (err) {
      console.error("[Updater] Relaunch failed:", err);
    }
  }, []);

  return {
    state,
    update,
    progress,
    error,
    checkForUpdates,
    installUpdate,
    relaunchApp,
  };
}
