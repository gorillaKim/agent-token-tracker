import { useEffect, useRef, useState } from "react";
import { useMcpServer } from "../hooks/useMcpServer";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Play,
  Square,
  Copy,
  Check,
  Terminal,
  Server,
  Settings,
  RefreshCw,
  FolderOpen
} from "lucide-react";
import { toast } from "sonner";

export function McpServerView() {
  const {
    status,
    logs,
    isActionLoading,
    startServer,
    stopServer,
    refreshStatus,
    clearLogs
  } = useMcpServer();

  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const logEndRef = useRef<HTMLDivElement | null>(null);

  // 로그 최하단 자동 스크롤
  useEffect(() => {
    if (logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs]);

  const handleCopy = (text: string, key: string) => {
    navigator.clipboard.writeText(text);
    setCopiedKey(key);
    toast.success("설정이 클립보드에 복사되었습니다.");
    setTimeout(() => setCopiedKey(null), 2000);
  };

  const getClaudeConfig = () => {
    if (!status) return "";
    const config = {
      mcpServers: {
        atk: {
          command: "agent-token-tracker",
          args: ["mcp", "--db", status.dbPath]
        }
      }
    };
    return JSON.stringify(config, null, 2);
  };

  const getAntigravityConfig = () => {
    if (!status) return "";
    return `plugins:\n  - name: "atk"\n    command: "agent-token-tracker"\n    args:\n      - "mcp"\n      - "--db"\n      - "${status.dbPath}"`;
  };

  return (
    <div className="flex flex-col gap-6 h-full pb-8">
      {/* 타이틀 및 상태 바 */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">MCP 서버 관리</h1>
          <p className="text-muted-foreground mt-1">
            ATK의 Model Context Protocol 서버를 켜거나 끄고 로그를 실시간으로 확인합니다.
          </p>
        </div>
        <Button variant="outline" size="icon" onClick={refreshStatus} disabled={isActionLoading}>
          <RefreshCw className={`h-4 w-4 ${isActionLoading ? "animate-spin" : ""}`} />
        </Button>
      </div>

      {/* 메인 레이아웃 */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 items-stretch">
        
        {/* 왼쪽: 컨트롤 및 가이드 */}
        <div className="lg:col-span-5 flex flex-col gap-6">
          {/* 서버 제어 카드 */}
          <Card className="border-border bg-card/40 backdrop-blur-md shadow-lg">
            <CardHeader className="pb-4">
              <div className="flex items-center justify-between">
                <CardTitle className="text-lg flex items-center gap-2">
                  <Server className="h-5 w-5 text-primary" />
                  서버 작동 상태
                </CardTitle>
                {status?.running ? (
                  <Badge variant="outline" className="bg-emerald-500/10 text-emerald-500 border-emerald-500/20 px-2.5 py-1 gap-1.5 font-semibold animate-pulse">
                    <span className="h-2 w-2 rounded-full bg-emerald-500"></span>
                    ON (PID: {status.pid})
                  </Badge>
                ) : (
                  <Badge variant="outline" className="bg-muted text-muted-foreground border-muted-foreground/20 px-2.5 py-1 gap-1.5 font-semibold">
                    <span className="h-2 w-2 rounded-full bg-muted-foreground/50"></span>
                    OFF
                  </Badge>
                )}
              </div>
              <CardDescription>
                로컬 IDE 에이전트가 ATK 데이터베이스를 조회할 수 있게 브리지 역할을 제공합니다.
              </CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col gap-4">
              <div className="flex gap-3">
                {status?.running ? (
                  <Button
                    variant="destructive"
                    className="flex-1 gap-2"
                    onClick={stopServer}
                    disabled={isActionLoading}
                  >
                    <Square className="h-4 w-4 fill-current" />
                    서버 중지
                  </Button>
                ) : (
                  <Button
                    className="flex-1 gap-2"
                    onClick={startServer}
                    disabled={isActionLoading}
                  >
                    <Play className="h-4 w-4 fill-current" />
                    서버 기동
                  </Button>
                )}
              </div>

              {status && (
                <div className="mt-2 space-y-2 text-sm">
                  <div className="flex items-center justify-between border-t border-border/40 pt-3">
                    <span className="text-muted-foreground">프로토콜 타입</span>
                    <span className="font-mono bg-accent px-1.5 py-0.5 rounded text-xs">stdio (Standard I/O)</span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground flex items-center gap-1">
                      <FolderOpen className="h-3.5 w-3.5" />
                      데이터베이스 경로
                    </span>
                    <span className="font-mono text-xs text-right break-all max-w-[200px] bg-accent px-1.5 py-0.5 rounded truncate" title={status.dbPath}>
                      {status.dbPath.split("/").pop()}
                    </span>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>

          {/* 연결 클라이언트 설정 가이드 */}
          <Card className="border-border bg-card/40 backdrop-blur-md shadow-lg flex-1">
            <CardHeader className="pb-2">
              <CardTitle className="text-lg flex items-center gap-2">
                <Settings className="h-5 w-5 text-primary" />
                에이전트 연결 설정
              </CardTitle>
              <CardDescription>
                사용하시는 에이전트(Claude Desktop 등) 설정 파일에 아래 구성을 추가하세요.
              </CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col gap-4 text-sm">
              
              {/* Claude Desktop */}
              <div className="space-y-2">
                <div className="flex justify-between items-center">
                  <span className="font-semibold text-xs text-foreground/80 uppercase tracking-wider">Claude Desktop (claude_desktop_config.json)</span>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 text-muted-foreground hover:text-foreground"
                    onClick={() => handleCopy(getClaudeConfig(), "claude")}
                  >
                    {copiedKey === "claude" ? <Check className="h-3.5 w-3.5 text-emerald-500" /> : <Copy className="h-3.5 w-3.5" />}
                  </Button>
                </div>
                <pre className="p-3 bg-zinc-950 text-zinc-300 rounded-lg text-xs font-mono overflow-x-auto max-h-[140px] border border-border/40">
                  {getClaudeConfig() || "서버 정보를 불러오는 중..."}
                </pre>
              </div>

              {/* Antigravity / Agent SDK */}
              <div className="space-y-2">
                <div className="flex justify-between items-center">
                  <span className="font-semibold text-xs text-foreground/80 uppercase tracking-wider">Antigravity / YAML 설정</span>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 text-muted-foreground hover:text-foreground"
                    onClick={() => handleCopy(getAntigravityConfig(), "antigravity")}
                  >
                    {copiedKey === "antigravity" ? <Check className="h-3.5 w-3.5 text-emerald-500" /> : <Copy className="h-3.5 w-3.5" />}
                  </Button>
                </div>
                <pre className="p-3 bg-zinc-950 text-zinc-300 rounded-lg text-xs font-mono overflow-x-auto max-h-[140px] border border-border/40">
                  {getAntigravityConfig() || "서버 정보를 불러오는 중..."}
                </pre>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* 오른쪽: 실시간 로그 모니터 */}
        <div className="lg:col-span-7 flex flex-col h-full min-h-[450px]">
          <Card className="border-border bg-card/30 backdrop-blur-md shadow-lg flex flex-col h-full overflow-hidden">
            <CardHeader className="pb-3 border-b border-border/40 flex flex-row items-center justify-between shrink-0">
              <div>
                <CardTitle className="text-lg flex items-center gap-2">
                  <Terminal className="h-5 w-5 text-primary" />
                  실시간 stderr 로그
                </CardTitle>
                <CardDescription>
                  stdio 입출력은 JSON-RPC 교환에 사용되므로, 진단 메시지는 stderr로 실시간 스트리밍됩니다.
                </CardDescription>
              </div>
              <Button variant="outline" size="sm" onClick={clearLogs} className="text-xs">
                지우기
              </Button>
            </CardHeader>
            <CardContent className="p-0 flex-1 min-h-0 bg-zinc-950">
              <ScrollArea className="h-[400px] w-full p-4 font-mono text-xs text-zinc-300 antialiased leading-relaxed">
                {logs.length === 0 ? (
                  <div className="h-full flex items-center justify-center text-zinc-500 italic py-20">
                    로그 메시지가 없습니다. MCP 서버가 기동되면 실시간 메시지가 표시됩니다.
                  </div>
                ) : (
                  <div className="space-y-1">
                    {logs.map((log, idx) => (
                      <div key={idx} className="whitespace-pre-wrap break-all hover:bg-zinc-900/60 py-0.5 px-1 rounded transition-colors">
                        {log}
                      </div>
                    ))}
                    <div ref={logEndRef} />
                  </div>
                )}
              </ScrollArea>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
