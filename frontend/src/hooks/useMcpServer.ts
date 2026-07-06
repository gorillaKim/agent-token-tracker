import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { McpServerStatus } from "../types";
import { toast } from "sonner";

export function useMcpServer() {
  const [status, setStatus] = useState<McpServerStatus | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [isActionLoading, setIsActionLoading] = useState(false);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  // 현재 상태 갱신
  const refreshStatus = useCallback(async () => {
    try {
      const res = await invoke<McpServerStatus>("mcp_server_status");
      setStatus(res);
      setLogs(res.logLines);
    } catch (e) {
      console.error("MCP 서버 상태 조회 실패:", e);
    }
  }, []);

  // 서버 시작
  const startServer = useCallback(async () => {
    setIsActionLoading(true);
    try {
      await invoke("mcp_server_start");
      toast.success("MCP 서버가 성공적으로 가동되었습니다.");
      await refreshStatus();
    } catch (e) {
      toast.error(`MCP 서버 시작 실패: ${e}`);
    } finally {
      setIsActionLoading(false);
    }
  }, [refreshStatus]);

  // 서버 중지
  const stopServer = useCallback(async () => {
    setIsActionLoading(true);
    try {
      await invoke("mcp_server_stop");
      toast.success("MCP 서버가 중지되었습니다.");
      await refreshStatus();
    } catch (e) {
      toast.error(`MCP 서버 중지 실패: ${e}`);
    } finally {
      setIsActionLoading(false);
    }
  }, [refreshStatus]);

  // 초기 상태 조회 및 이벤트 구독
  useEffect(() => {
    refreshStatus();

    // Tauri "mcp-log" 이벤트 실시간 리스너 등록
    listen<string>("mcp-log", (event) => {
      setLogs((prev) => {
        const next = [...prev, event.payload];
        if (next.length > 500) {
          next.shift();
        }
        return next;
      });
    }).then((unlisten) => {
      unlistenRef.current = unlisten;
    });

    return () => {
      if (unlistenRef.current) {
        unlistenRef.current();
      }
    };
  }, [refreshStatus]);

  return {
    status,
    logs,
    isActionLoading,
    startServer,
    stopServer,
    refreshStatus,
    clearLogs: useCallback(() => setLogs([]), []),
  };
}
