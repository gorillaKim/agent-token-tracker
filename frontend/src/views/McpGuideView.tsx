import { useState } from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  BookOpen,
  Coins,
  Cpu,
  ShieldAlert,
  Code,
  Copy,
  Check,
  ChevronRight,
  Terminal,
  Info
} from "lucide-react";
import { toast } from "sonner";

interface ToolArg {
  name: string;
  type: string;
  required: boolean;
  desc: string;
}

interface ToolDoc {
  name: string;
  desc: string;
  args: ToolArg[];
  usage: string;
  response: string;
}

export function McpGuideView() {
  const [activeCategory, setActiveCategory] = useState<"token" | "analysis" | "anomaly">("token");
  const [selectedToolName, setSelectedToolName] = useState<string>("get_token_summary");
  const [copiedText, setCopiedText] = useState<string | null>(null);

  const categories = [
    { key: "token", label: "토큰 & 비용 모니터링", icon: Coins, color: "text-amber-500 bg-amber-500/10" },
    { key: "analysis", label: "도구 & 플러그인 분석", icon: Cpu, color: "text-blue-500 bg-blue-500/10" },
    { key: "anomaly", label: "루프 & 오작동 탐지", icon: ShieldAlert, color: "text-rose-500 bg-rose-500/10" },
  ];

  const toolsData: Record<string, ToolDoc[]> = {
    token: [
      {
        name: "get_token_summary",
        desc: "에이전트 타입(codex, claude_code, antigravity)별 총 세션 수, 입력/출력 토큰량 및 누적 소모 비용을 집계합니다.",
        args: [
          { name: "since", type: "String", required: false, desc: "조회 시작일 필터 (예: '2026-07-01'). 지정하지 않으면 전체 기간 집계." },
          { name: "limit", type: "Integer", required: false, desc: "출력할 최대 에이전트 종류 개수." }
        ],
        usage: `{ "since": "2026-07-01", "limit": 10 }`,
        response: `## 에이전트별 토큰 요약 *(since 2026-07-01)*\n\n| 에이전트 | 세션 | 입력 | 출력 | 비용 |\n|---|---|---|---|---|\n| \x60claude_code\x60 | 12 | 1.2M | 350K | $7.85 |\n| \x60antigravity\x60 | 5 | 450K | 92K | $2.55 |\n\n**합계** — 입력 1.6M / 출력 442K / 비용 $10.40`
      },
      {
        name: "get_session_report",
        desc: "개별 세션 단위의 상세 리포트를 조회합니다. 누적 토큰, 비용, 실행 일자 및 모델 정보를 조회할 수 있습니다.",
        args: [
          { name: "session_id", type: "String", required: false, desc: "특정 세션 ID 필터. 지정 시 해당 세션만 단독 조회." },
          { name: "since", type: "String", required: false, desc: "조회 시작일 필터 (예: '2026-07-01')." },
          { name: "sort", type: "String", required: false, desc: "정렬 기준: 'cost'(비용), 'tokens'(토큰). 기본값은 시작시각 내림차순." },
          { name: "limit", type: "Integer", required: false, desc: "조회할 최대 세션 개수." }
        ],
        usage: `{ "since": "2026-07-01", "sort": "cost", "limit": 5 }`,
        response: `## 세션 리포트 *(since 2026-07-01)*\n\n| ID | 에이전트 | 모델 | 입력 | 출력 | 비용 | 시작 |\n|---|---|---|---|---|---|---|\n| \x60a9b8c7d6\x60 | \x60claude_code\x60 | claude-3-5-sonnet | 250K | 45K | $1.4250 | 2026-07-06 |\n| \x60f1e2d3c4\x60 | \x60antigravity\x60 | gpt-4o | 120K | 32K | $1.0800 | 2026-07-05 |`
      },
      {
        name: "get_today_usage",
        desc: "오늘(UTC 기준) 활성화되었거나 시작된 세션들의 토큰 사용량과 소모 비용 현황을 신속하게 요약하여 반환합니다.",
        args: [],
        usage: `{}`,
        response: `## 오늘 사용량 (2026-07-07)\n\n - 세션: **3개**\n - 입력 토큰: **45.5K**\n - 출력 토큰: **12.3K**\n - 총 비용: **$0.3208**\n\n| ID | 에이전트 | 모델 | 입력 | 출력 | 비용 |\n|---|---|---|---|---|---|\n| \x60d6f7e8a9\x60 | \x60claude_code\x60 | claude-3-5-sonnet | 32K | 8K | $0.2160 |`
      },
      {
        name: "search_sessions",
        desc: "작업 디렉토리 경로(cwd)를 기준으로 세션을 검색하여 해당 작업 공간에서 발생한 누적 소모량을 보여줍니다.",
        args: [
          { name: "cwd_contains", type: "String", required: true, desc: "검색할 디렉토리 경로에 포함될 문자열 (예: 'gorillaProject')." },
          { name: "since", type: "String", required: false, desc: "조회 시작일 필터." },
          { name: "limit", type: "Integer", required: false, desc: "최대 검색 결과 개수. 기본값: 30." }
        ],
        usage: `{ "cwd_contains": "gorillaProject", "limit": 5 }`,
        response: `## 세션 검색: \x60gorillaProject\x60 (2개)\n\n| ID | 에이전트 | 모델 | 입력 | 출력 | 비용 | 시작 |\n|---|---|---|---|---|---|---|\n| \x60bc12de34\x60 | \x60claude_code\x60 | claude-3-5-sonnet | 89K | 21K | $0.5820 | 2026-07-06 |`
      }
    ],
    analysis: [
      {
        name: "get_tool_usage",
        desc: "에이전트가 호출한 도구별 호출 횟수, 성공률, 루프 의심 횟수, 그리고 반환 결과의 크기를 분석해 집계합니다.",
        args: [
          { name: "since", type: "String", required: false, desc: "조회 시작일 필터." },
          { name: "sort", type: "String", required: false, desc: "정렬 기준: 'count'(호출 수), 'loop'(루프의심). 기본값은 호출 수." },
          { name: "limit", type: "Integer", required: false, desc: "조회할 도구 최대 개수." }
        ],
        usage: `{ "sort": "count", "limit": 10 }`,
        response: `## 도구 사용 통계\n\n| 도구 | 호출 | 성공률 | 루프의심 | 결과 토큰 (추정 합/평균) |\n|---|---|---|---|---|\n| \x60run_command\x60 | 42 | 90% | 2 ⚠️ | 150K / 3.5K |\n| \x60view_file\x60 | 28 | 100% | 0 | 85K / 3.0K |`
      },
      {
        name: "get_mcp_plugin_summary",
        desc: "에이전트가 연동하여 호출한 타 MCP 서버(플러그인)별 사용량과 소모 비용을 모니터링합니다.",
        args: [
          { name: "since", type: "String", required: false, desc: "조회 시작일 필터." },
          { name: "limit", type: "Integer", required: false, desc: "최대 조회 개수." }
        ],
        usage: `{ "since": "2026-07-01" }`,
        response: `## MCP 플러그인 사용량 *(since 2026-07-01)*\n\n| 서버 | 호출 | 성공률 | 루프⚠️ | 세션 | 결과토큰(추정 합) | 입력(세션귀속⚠️) | 출력(세션귀속⚠️) | 비용(세션귀속⚠️) |\n|---|---|---|---|---|---|---|---|---|\n| \x60engram\x60 | 120 | 95% | 1 ⚠️ | 4 | 220K | 850K | 190K | $5.40 |\n| \x60playwright\x60 | 45 | 88% | 0 | 2 | 120K | 320K | 75K | $2.08 |`
      },
      {
        name: "get_mcp_plugin_tools",
        desc: "특정 MCP 서버 내 개별 도구들의 사용 빈도 및 추정 토큰을 세부적으로 들여다봅니다.",
        args: [
          { name: "mcp_server", type: "String", required: true, desc: "조회할 대상 MCP 서버 이름 (예: 'engram')." },
          { name: "since", type: "String", required: false, desc: "조회 시작일 필터." },
          { name: "sort", type: "String", required: false, desc: "정렬 기준: 'count'(호출 수), 'tokens'(결과토큰), 'cost'(비용)." },
          { name: "limit", type: "Integer", required: false, desc: "최대 조회 개수." }
        ],
        usage: `{ "mcp_server": "engram", "sort": "count" }`,
        response: `## \x60engram\x60 도구별 사용량\n\n| 도구 | 호출 | 성공률 | 루프⚠️ | 세션 | 결과토큰(추정 합/평균) | 입력(세션귀속⚠️) | 출력(세션귀속⚠️) | 비용(세션귀속⚠️) |\n|---|---|---|---|---|---|---|---|---|\n| \x60issue_get\x60 | 48 | 100% | 0 | 3 | 55K / 1.1K | 450K | 89K | $2.68 |`
      },
      {
        name: "get_tool_trend",
        desc: "도구별로 생성해내는 결과 토큰 크기의 일별 평균 시계열 추세를 반환하여 과다 페이로드 생성 패턴을 모니터링합니다.",
        args: [
          { name: "since", type: "String", required: false, desc: "조회 시작일 필터." }
        ],
        usage: `{ "since": "2026-07-01" }`,
        response: `## 도구 결과 토큰 시계열 추세\n\n| 날짜 | 도구명 | 평균 결과 토큰 | 호출 횟수 |\n|---|---|---|---|\n| 2026-07-06 | \x60read_file\x60 | 8.2K | 15 |\n| 2026-07-06 | \x60run_command\x60 | 4.1K | 22 |`
      },
      {
        name: "get_tool_offenders",
        desc: "결과 데이터 크기가 비정상적으로 컸던 최악의 도구 호출 사례(Top-N)를 추려내어 페이로드 최적화 진단을 돕습니다.",
        args: [
          { name: "since", type: "String", required: false, desc: "조회 시작일 필터." },
          { name: "limit", type: "Integer", required: false, desc: "추려낼 Top-N 개수. 기본값: 10." }
        ],
        usage: `{ "limit": 5 }`,
        response: `## 도구 결과 오펜더 랭킹\n\n| 세션 ID | 도구명 | 일시 | 결과 글자수 | 결과 추정 토큰 |\n|---|---|---|---|---|\n| \x60a9b8c7d6\x60 | \x60run_command\x60 | 2026-07-06 14:22:15 | 385,200 | 96.3K |`
      },
      {
        name: "get_tool_percentiles",
        desc: "각 도구별 반환 결과 크기의 백분위 분포(p50, p90, Max)를 조회해 이상적으로 큰 응답이 발생하는 도구를 식별합니다.",
        args: [
          { name: "since", type: "String", required: false, desc: "조회 시작일 필터." }
        ],
        usage: `{}`,
        response: `## 도구 결과 백분위 분포\n\n| 도구명 | 호출수 | p50 토큰 | p90 토큰 | Max 토큰 |\n|---|---|---|---|---|\n| \x60run_command\x60 | 42 | 1.2K | 12.5K | 96.3K |\n| \x60view_file\x60 | 28 | 800 | 3.5K | 12.0K |`
      }
    ],
    anomaly: [
      {
        name: "get_loop_suspects",
        desc: "동일 도구 반복 호출, 무한 핑퐁, 비정상적 토큰 급증 등 루프 의심 상태로 지목된 세션들을 선별합니다.",
        args: [
          { name: "agent_type", type: "String", required: false, desc: "특정 에이전트 종류 필터." },
          { name: "since", type: "String", required: false, desc: "조회 시작일 필터." },
          { name: "limit", type: "Integer", required: false, desc: "최대 결과 개수. 기본값: 20." }
        ],
        usage: `{ "limit": 3 }`,
        response: `## ⚠️ 루프 의심 세션 (2개)\n\n| ID | 에이전트 | 루프 호출 수 | 시작 |\n|---|---|---|---|\n| \x60z1x2c3v4\x60 | \x60claude_code\x60 | **8** | 2026-07-06 |\n| \x60y9t8r7e6\x60 | \x60antigravity\x60 | **5** | 2026-07-05 |`
      },
      {
        name: "register_malfunction_pattern",
        desc: "답변 지연 시간, 반복 실패, 특정 플러그인 한계치, 정규식 에러 등 고유의 오작동 매칭 규칙(Pattern)을 새로 생성/등록합니다.",
        args: [
          { name: "pattern_name", type: "String", required: true, desc: "오작동 패턴을 식별할 고유 명칭 (예: '지연 및 비정상 종료')." },
          { name: "description", type: "String", required: false, desc: "패턴 동작 조건에 대한 상세 설명." },
          { name: "rules_json", type: "String", required: true, desc: "MalfunctionRule Enum이 직렬화된 JSON 규칙 정의 문자열." }
        ],
        usage: `{ "pattern_name": "심각한 응답 지연", "description": "응답 속도가 120초 이상 지연된 사례", "rules_json": "{\\"type\\":\\"max_response_delay_sec\\",\\"value\\":120}" }`,
        response: `✅ 오작동 패턴 등록 완료 (ID: 3)`
      },
      {
        name: "get_malfunction_patterns",
        desc: "현재 시스템에 등록되어 있는 모든 오작동 패턴 매칭 규칙들의 정의와 상세 내용을 조회합니다.",
        args: [],
        usage: `{}`,
        response: `## 🔍 등록된 오작동 감지 패턴 목록\n\n| ID | 패턴명 | 설명 | 규칙 요약 | 등록 시간 |\n|---|---|---|---|---|\n| 1 | **핑퐁 감지** | 두 도구간 연속 핑퐁 | \x60{"type":"dynamic_ping_pong","cycles_threshold"...\x60 | 2026-07-07 |`
      },
      {
        name: "analyze_session_malfunctions",
        desc: "특정 대화 세션에 대해 현재 등록된 모든 오작동 패턴을 대조해 실시간 매칭 분석을 수행하고, 감지 이력을 적재 및 반환합니다.",
        args: [
          { name: "session_id", type: "String", required: true, desc: "분석할 대상 세션 ID." }
        ],
        usage: `{ "session_id": "test-sess-123" }`,
        response: `## ⚠️ 세션 \x60test-sess\x60 오작동 감지 이력 (1건)\n\n| ID | 패턴명 | 설명 | 상세 증거 (Evidence) | 감지 시각 |\n|---|---|---|---|---|\n| 5 | **핑퐁 감지** | 두 도구간 연속 핑퐁 | 임의의 두 도구 간 핑퐁 감지: 실제 왕복 3회 (임계치 2회) | 2026-07-07 |`
      },
      {
        name: "get_session_malfunctions",
        desc: "특정 세션에서 기존에 이미 분석/적재되었던 모든 오작동 감지 이력을 조회합니다.",
        args: [
          { name: "session_id", type: "String", required: true, desc: "조회할 대상 세션 ID." }
        ],
        usage: `{ "session_id": "test-sess-123" }`,
        response: `## ⚠️ 세션 \x60test-sess\x60 오작동 감지 이력 (1건)\n\n| ID | 패턴명 | 설명 | 상세 증거 (Evidence) | 감지 시각 |\n|---|---|---|---|---|\n| 5 | **핑퐁 감지** | 두 도구간 연속 핑퐁 | 임의의 두 도구 간 핑퐁 감지: 실제 왕복 3회 (임계치 2회) | 2026-07-07 |`
      },
      {
        name: "delete_malfunction_pattern",
        desc: "더 이상 사용하지 않는 오작동 감지 패턴을 데이터베이스에서 제거합니다.",
        args: [
          { name: "id", type: "Integer", required: true, desc: "삭제할 패턴의 고유 ID." }
        ],
        usage: `{ "id": 3 }`,
        response: `✅ 오작동 패턴 삭제 완료 (ID: 3)`
      }
    ]
  };

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    setCopiedText(text);
    toast.success("예제 코드가 클립보드에 복사되었습니다.");
    setTimeout(() => setCopiedText(null), 2000);
  };

  const currentCategoryTools = toolsData[activeCategory] || [];
  const selectedTool = currentCategoryTools.find(t => t.name === selectedToolName) || currentCategoryTools[0];

  const handleCategoryChange = (key: "token" | "analysis" | "anomaly") => {
    setActiveCategory(key);
    const categoryTools = toolsData[key] || [];
    if (categoryTools.length > 0) {
      setSelectedToolName(categoryTools[0].name);
    }
  };

  return (
    <div className="flex flex-col gap-6 h-full pb-8">
      <div>
        <h1 className="text-3xl font-bold tracking-tight flex items-center gap-2.5">
          <BookOpen className="h-8 w-8 text-primary" />
          MCP 도구 설명 가이드
        </h1>
        <p className="text-muted-foreground mt-1">
          ATK MCP 서버가 에이전트에 노출하는 도구(Tool)들의 명세, 인자 정보 및 마크다운 응답 예시를 파악합니다.
        </p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 items-stretch">
        <div className="lg:col-span-4 flex flex-col gap-4">
          <div className="flex flex-col gap-2 p-1.5 bg-card/30 border border-border/60 rounded-xl backdrop-blur-md">
            {categories.map((cat) => {
              const Icon = cat.icon;
              const active = activeCategory === cat.key;
              return (
                <button
                  key={cat.key}
                  onClick={() => handleCategoryChange(cat.key as any)}
                  className={`flex items-center gap-3 w-full px-3.5 py-2.5 rounded-lg text-sm font-medium transition-all ${
                    active
                      ? "bg-primary text-primary-foreground shadow-md"
                      : "text-muted-foreground hover:text-foreground hover:bg-accent/40"
                  }`}
                >
                  <div className={`p-1.5 rounded-md ${active ? "bg-white/15 text-white" : cat.color}`}>
                    <Icon className="h-4 w-4" />
                  </div>
                  <span>{cat.label}</span>
                </button>
              );
            })}
          </div>

          <Card className="border-border bg-card/40 backdrop-blur-md shadow-lg flex-1">
            <CardHeader className="py-4">
              <CardTitle className="text-sm font-semibold uppercase tracking-wider text-muted-foreground/80">도구 목록</CardTitle>
            </CardHeader>
            <CardContent className="p-0">
              <ScrollArea className="h-[280px] lg:h-[350px]">
                <div className="flex flex-col p-2 gap-1">
                  {currentCategoryTools.map((tool) => {
                    const active = selectedToolName === tool.name;
                    return (
                      <button
                        key={tool.name}
                        onClick={() => setSelectedToolName(tool.name)}
                        className={`flex items-center justify-between w-full px-3 py-2.5 rounded-lg text-left text-xs transition-colors ${
                          active
                            ? "bg-accent text-foreground font-semibold border-l-2 border-primary pl-2.5"
                            : "text-muted-foreground hover:text-foreground hover:bg-accent/40"
                        }`}
                      >
                        <span className="font-mono">{tool.name}</span>
                        <ChevronRight className={`h-3 w-3 transition-transform ${active ? "text-primary translate-x-0.5" : "text-muted-foreground/40"}`} />
                      </button>
                    );
                  })}
                </div>
              </ScrollArea>
            </CardContent>
          </Card>
        </div>

        <div className="lg:col-span-8 flex flex-col min-h-[500px]">
          {selectedTool ? (
            <Card className="border-border bg-card/30 backdrop-blur-md shadow-lg flex flex-col h-full overflow-hidden">
              <CardHeader className="pb-3 border-b border-border/40 shrink-0">
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="outline" className="bg-primary/5 text-primary border-primary/20 font-mono py-0.5 px-2">
                    {selectedTool.name}
                  </Badge>
                  <Badge variant="secondary" className="text-[10px] font-semibold tracking-wider">
                    {activeCategory === "token" ? "MONITORING" : activeCategory === "analysis" ? "METRICS" : "DETECTION"}
                  </Badge>
                </div>
                <CardTitle className="text-xl font-bold tracking-tight mt-2 font-mono">
                  {selectedTool.name}
                </CardTitle>
                <CardDescription className="text-sm text-foreground/80 mt-1.5 leading-relaxed">
                  {selectedTool.desc}
                </CardDescription>
              </CardHeader>
              
              <ScrollArea className="flex-1 bg-zinc-950/20">
                <CardContent className="p-6 flex flex-col gap-6 text-sm">
                  <div>
                    <h3 className="text-xs font-bold uppercase tracking-wider text-muted-foreground/80 mb-2 flex items-center gap-1.5">
                      <Info className="h-3.5 w-3.5" />
                      호출 인자 (Arguments)
                    </h3>
                    {selectedTool.args.length === 0 ? (
                      <div className="p-3 bg-card/20 rounded-lg text-xs text-muted-foreground italic border border-border/30">
                        인자가 필요하지 않은 도구입니다.
                      </div>
                    ) : (
                      <div className="border border-border/55 rounded-lg overflow-hidden bg-card/10">
                        <div className="grid grid-cols-12 bg-card/40 px-3 py-2 text-xs font-semibold border-b border-border/55 text-muted-foreground/80 uppercase">
                          <div className="col-span-3">인자명</div>
                          <div className="col-span-2">타입</div>
                          <div className="col-span-2">필수여부</div>
                          <div className="col-span-5">설명</div>
                        </div>
                        <div className="divide-y divide-border/55">
                          {selectedTool.args.map((arg) => (
                            <div key={arg.name} className="grid grid-cols-12 px-3 py-2.5 text-xs items-start font-medium">
                              <div className="col-span-3 font-mono text-foreground font-semibold">{arg.name}</div>
                              <div className="col-span-2 font-mono text-muted-foreground">{arg.type}</div>
                              <div className="col-span-2">
                                {arg.required ? (
                                  <Badge className="bg-rose-500/10 text-rose-500 hover:bg-rose-500/15 border-rose-500/20 py-0 text-[10px] scale-90 -translate-x-1 font-semibold">REQUIRED</Badge>
                                ) : (
                                  <Badge variant="outline" className="text-muted-foreground py-0 text-[10px] scale-90 -translate-x-1 font-semibold">OPTIONAL</Badge>
                                )}
                              </div>
                              <div className="col-span-5 text-muted-foreground leading-relaxed">{arg.desc}</div>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>

                  <div>
                    <div className="flex justify-between items-center mb-2">
                      <h3 className="text-xs font-bold uppercase tracking-wider text-muted-foreground/80 flex items-center gap-1.5">
                        <Code className="h-3.5 w-3.5" />
                        호출 Payload 예제 (JSON)
                      </h3>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-6 w-6 text-muted-foreground hover:text-foreground"
                        onClick={() => handleCopy(selectedTool.usage)}
                      >
                        {copiedText === selectedTool.usage ? (
                          <Check className="h-3.5 w-3.5 text-emerald-500" />
                        ) : (
                          <Copy className="h-3.5 w-3.5" />
                        )}
                      </Button>
                    </div>
                    <pre className="p-3.5 bg-zinc-950 text-zinc-300 rounded-lg text-xs font-mono overflow-x-auto border border-border/40">
                      {selectedTool.usage}
                    </pre>
                  </div>

                  <div>
                    <h3 className="text-xs font-bold uppercase tracking-wider text-muted-foreground/80 mb-2 flex items-center gap-1.5">
                      <Terminal className="h-3.5 w-3.5" />
                      마크다운 응답 예시 (Markdown Response)
                    </h3>
                    <pre className="p-4 bg-zinc-950/70 text-emerald-400/90 rounded-lg text-xs font-mono overflow-x-auto whitespace-pre border border-border/40 max-h-[300px]">
                      {selectedTool.response}
                    </pre>
                  </div>
                </CardContent>
              </ScrollArea>
            </Card>
          ) : (
            <div className="flex-1 flex items-center justify-center border border-dashed border-border/60 rounded-xl p-10 text-muted-foreground italic">
              도구를 선택하시면 명세가 표시됩니다.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
