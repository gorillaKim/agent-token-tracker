import { useMemo, useState } from "react";
import { CalendarDays, ChevronLeft, ChevronRight, Puzzle, Wrench } from "lucide-react";
import { CostRankItem, DailyUsageDetail } from "../types";
import { useCalendarData, useDayCostBreakdown } from "../hooks/useCalendarData";
import { formatTokens, formatUsd } from "../utils/formatters";
import {
  getMonthGrid,
  localCurrentYearMonth,
  localTodayKey,
  monthLabel,
  dateLabel,
  WEEKDAY_LABELS,
} from "../utils/calendar";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

type DisplayMode = "tokens" | "cost";

/** 에이전트별 분해 표시용 메타 (색상 토큰 + 데이터 키) */
const AGENTS: {
  label: string;
  text: string;
  bar: string;
  tokenKey: keyof DailyUsageDetail;
  costKey: keyof DailyUsageDetail;
}[] = [
  { label: "Claude Code", text: "text-agent-claude", bar: "bg-agent-claude", tokenKey: "claude_tokens", costKey: "claude_cost" },
  { label: "Codex (OpenAI)", text: "text-agent-codex", bar: "bg-agent-codex", tokenKey: "codex_tokens", costKey: "codex_cost" },
  { label: "Antigravity", text: "text-agent-antigravity", bar: "bg-agent-antigravity", tokenKey: "antigravity_tokens", costKey: "antigravity_cost" },
];

/** 히트맵 강도 단계별 배경 불투명도 (0값 제외 5단계) */
const HEAT_ALPHAS = [0.1, 0.22, 0.34, 0.46, 0.6];

/** 선택 모드의 일별 값 추출 */
function metricOf(rec: DailyUsageDetail, mode: DisplayMode): number {
  return mode === "tokens" ? rec.total_tokens : rec.total_cost;
}

/** 모드에 맞춘 값 포맷 */
function formatMetric(value: number, mode: DisplayMode): string {
  return mode === "tokens" ? formatTokens(value) : `$${formatUsd(value)}`;
}

const MODE_OPTIONS: { v: DisplayMode; label: string }[] = [
  { v: "tokens", label: "토큰" },
  { v: "cost", label: "비용 (USD)" },
];

/** 토큰 / 비용 세그먼트 토글 (캘린더 헤더 + 상세 모달에서 공용 사용) */
function ModeToggle({
  mode,
  setMode,
  size = "default",
}: {
  mode: DisplayMode;
  setMode: (m: DisplayMode) => void;
  size?: "default" | "sm";
}) {
  return (
    <div className="inline-flex w-fit items-center gap-0.5 rounded-lg border border-border bg-muted/40 p-0.5">
      {MODE_OPTIONS.map((opt) => (
        <button
          key={opt.v}
          onClick={() => setMode(opt.v)}
          className={cn(
            "rounded-md font-medium transition-colors",
            size === "sm" ? "px-2.5 py-1 text-xs" : "px-3 py-1.5 text-sm",
            mode === opt.v
              ? "bg-background text-foreground shadow-sm"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}

/** 사용량 랭킹 섹션 (플러그인별 / 도구별) — 표시 모드(토큰/비용)에 맞춰 정렬·표시 */
function RankSection({
  title,
  icon: Icon,
  items,
  loading,
  mode,
}: {
  title: string;
  icon: React.ComponentType<{ className?: string }>;
  items?: CostRankItem[];
  loading: boolean;
  mode: DisplayMode;
}) {
  const valueOf = (i: CostRankItem) => (mode === "tokens" ? i.total_tokens : i.total_cost);
  // 선택 모드 기준 내림차순 정렬 후 상위 10개
  const ranked = (items ?? [])
    .slice()
    .sort((a, b) => valueOf(b) - valueOf(a))
    .slice(0, 10);
  const max = ranked.length ? valueOf(ranked[0]) : 0;

  return (
    <TooltipProvider delayDuration={200}>
      <div className="flex flex-col gap-2">
        <h4 className="flex items-center gap-1.5 text-sm font-semibold">
          <Icon className="h-4 w-4 text-muted-foreground" />
          {title}
        </h4>
        {loading ? (
          <div className="py-4 text-center text-xs text-muted-foreground">불러오는 중…</div>
        ) : ranked.length === 0 ? (
          <div className="py-4 text-center text-xs text-muted-foreground">집계된 사용량이 없습니다.</div>
        ) : (
          <div className="flex flex-col gap-1.5">
            {ranked.map((item, idx) => {
              const val = valueOf(item);
              const pct = max > 0 ? (val / max) * 100 : 0;
              return (
                <div key={item.name} className="flex flex-col gap-1">
                  <div className="flex items-center justify-between gap-2 text-xs">
                    <span className="flex min-w-0 items-center gap-1.5">
                      <span className="w-4 shrink-0 text-right tabular-nums text-muted-foreground/60">
                        {idx + 1}
                      </span>
                      {/* 말줄임 시 마우스 오버로 풀네임 툴팁 노출 */}
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <span className="min-w-0 cursor-default truncate font-medium">
                            {item.name}
                          </span>
                        </TooltipTrigger>
                        <TooltipContent className="max-w-[340px] break-all font-mono text-xs">
                          {item.name}
                        </TooltipContent>
                      </Tooltip>
                    </span>
                    <span className="shrink-0 tabular-nums">
                      <span className={cn(mode === "cost" ? "text-primary" : "text-foreground")}>
                        {formatMetric(val, mode)}
                      </span>
                      <span className="ml-1.5 text-muted-foreground/70">{item.call_count}회</span>
                    </span>
                  </div>
                  <div className="h-1 overflow-hidden rounded-full bg-muted">
                    <div className="h-full rounded-full bg-primary/70" style={{ width: `${pct}%` }} />
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </TooltipProvider>
  );
}

/**
 * 사용량 캘린더 뷰
 *
 * 월간 그리드 위에 일별 토큰/비용 사용량을 히트맵 강도로 표시하고, 이전/다음 달로 과거를 탐색한다.
 * 상단 토글로 토큰 ↔ 비용(USD) 표시를 전환하며(기본값: 토큰), 날짜를 클릭하면 그날의 에이전트별 상세를 보여준다.
 */
export function CalendarView() {
  const [ym, setYm] = useState<{ year: number; month: number }>(() => localCurrentYearMonth());
  const [mode, setMode] = useState<DisplayMode>("tokens");
  const [selectedDate, setSelectedDate] = useState<string | null>(null);

  const { byDate, loading } = useCalendarData(ym.year, ym.month);
  const { data: dayBreakdown, loading: breakdownLoading } = useDayCostBreakdown(selectedDate);
  const grid = useMemo(() => getMonthGrid(ym.year, ym.month), [ym]);
  const todayKey = localTodayKey();

  // 이번 달 in-month 셀들의 선택 모드 최댓값 (히트맵 정규화 기준)
  const maxVal = useMemo(() => {
    let max = 0;
    for (const cell of grid) {
      if (!cell.inMonth) continue;
      const rec = byDate.get(cell.date);
      if (rec) max = Math.max(max, metricOf(rec, mode));
    }
    return max;
  }, [grid, byDate, mode]);

  // 이번 달 합계 (헤더 요약용)
  const monthTotals = useMemo(() => {
    let tokens = 0;
    let cost = 0;
    for (const cell of grid) {
      if (!cell.inMonth) continue;
      const rec = byDate.get(cell.date);
      if (rec) {
        tokens += rec.total_tokens;
        cost += rec.total_cost;
      }
    }
    return { tokens, cost };
  }, [grid, byDate]);

  const goPrev = () =>
    setYm(({ year, month }) => (month === 0 ? { year: year - 1, month: 11 } : { year, month: month - 1 }));
  const goNext = () =>
    setYm(({ year, month }) => (month === 11 ? { year: year + 1, month: 0 } : { year, month: month + 1 }));
  const goToday = () => setYm(localCurrentYearMonth());

  const heatStyle = (value: number): React.CSSProperties => {
    if (value <= 0 || maxVal <= 0) return {};
    const ratio = value / maxVal;
    const idx = Math.min(HEAT_ALPHAS.length - 1, Math.max(0, Math.ceil(ratio * HEAT_ALPHAS.length) - 1));
    return { backgroundColor: `hsl(var(--primary) / ${HEAT_ALPHAS[idx]})` };
  };

  const selectedRec = selectedDate ? byDate.get(selectedDate) : undefined;

  return (
    <div className="flex flex-col gap-6">
      {/* 헤더: 월 네비게이터 + 토큰/비용 토글 */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex items-center gap-2">
          <CalendarDays className="h-5 w-5 text-muted-foreground" />
          <h2 className="text-xl font-semibold tracking-tight tabular-nums">
            {monthLabel(ym.year, ym.month)}
          </h2>
          <div className="ml-2 flex items-center gap-1">
            <Button variant="ghost" size="icon-sm" onClick={goPrev} title="이전 달">
              <ChevronLeft className="h-4 w-4" />
            </Button>
            <Button variant="ghost" size="icon-sm" onClick={goNext} title="다음 달">
              <ChevronRight className="h-4 w-4" />
            </Button>
            <Button variant="outline" size="sm" onClick={goToday}>
              오늘
            </Button>
          </div>
        </div>

        {/* 토큰 / 비용 토글 */}
        <ModeToggle mode={mode} setMode={setMode} />
      </div>

      {/* 월 합계 요약 바 */}
      <div className="flex flex-wrap items-center gap-x-8 gap-y-2 rounded-xl border border-border bg-card/40 px-5 py-3.5">
        <div className="flex flex-col">
          <span className="text-xs text-muted-foreground">이번 달 총 토큰</span>
          <span className="text-lg font-semibold tabular-nums">
            {formatTokens(monthTotals.tokens)}
            <span className="ml-1 text-sm font-normal text-muted-foreground">Tokens</span>
          </span>
        </div>
        <div className="flex flex-col">
          <span className="text-xs text-muted-foreground">이번 달 추정 비용</span>
          <span className="text-lg font-semibold tabular-nums text-primary">
            ${formatUsd(monthTotals.cost)}
            <span className="ml-1 text-sm font-normal text-muted-foreground">USD</span>
          </span>
        </div>

        {/* 에이전트 색상 범례 (셀의 미니 분해 점 색상 해설) */}
        <div className="ml-auto flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
          {AGENTS.map((agent) => (
            <span key={agent.label} className="flex items-center gap-1.5">
              <span className={cn("h-2 w-2 rounded-full", agent.bar)} />
              {agent.label}
            </span>
          ))}
          <span className="flex items-center gap-1.5">
            <span className="font-semibold text-foreground">∑</span>
            합계
          </span>
        </div>
        {loading && <span className="text-xs text-muted-foreground">불러오는 중…</span>}
      </div>

      {/* 캘린더 카드 */}
      <Card>
        <CardContent className="pt-6">
          {/* 요일 헤더 */}
          <div className="mb-2 grid grid-cols-7 gap-2">
            {WEEKDAY_LABELS.map((w, i) => (
              <div
                key={w}
                className={cn(
                  "text-center text-xs font-medium text-muted-foreground",
                  i === 0 && "text-destructive/80",
                  i === 6 && "text-agent-codex/80"
                )}
              >
                {w}
              </div>
            ))}
          </div>

          {/* 날짜 그리드 */}
          <div className="grid grid-cols-7 gap-2">
            {grid.map((cell) => {
              const rec = byDate.get(cell.date);
              const value = rec ? metricOf(rec, mode) : 0;
              const hasUsage = !!rec && rec.total_tokens + rec.total_cost > 0;
              const isToday = cell.date === todayKey;
              const isSelected = cell.date === selectedDate;

              return (
                <button
                  key={cell.date}
                  type="button"
                  disabled={!cell.inMonth}
                  onClick={() => setSelectedDate(cell.date)}
                  style={cell.inMonth ? heatStyle(value) : undefined}
                  className={cn(
                    "flex min-h-[92px] flex-col rounded-md border p-1.5 text-left transition-colors",
                    cell.inMonth
                      ? "border-border bg-card/40 hover:border-primary/60"
                      : "pointer-events-none border-transparent opacity-30",
                    isToday && "ring-1 ring-primary",
                    isSelected && "border-primary"
                  )}
                >
                  <span
                    className={cn(
                      "self-end text-xs tabular-nums",
                      isToday ? "font-semibold text-primary" : "text-muted-foreground"
                    )}
                  >
                    {cell.day}
                  </span>

                  {/* 에이전트별 + 총계 미니 분해 */}
                  {cell.inMonth && hasUsage && rec && (
                    <div className="mt-auto flex flex-col gap-px">
                      {AGENTS.map((agent) => {
                        const av = (mode === "tokens"
                          ? rec[agent.tokenKey]
                          : rec[agent.costKey]) as number;
                        return (
                          <div
                            key={agent.label}
                            className="flex items-center justify-between gap-1 text-[10px] leading-tight"
                          >
                            <span
                              className={cn(
                                "h-1.5 w-1.5 shrink-0 rounded-full",
                                agent.bar,
                                av > 0 ? "opacity-100" : "opacity-25"
                              )}
                            />
                            <span
                              className={cn(
                                "tabular-nums",
                                av > 0 ? "text-foreground/90" : "text-muted-foreground/40"
                              )}
                            >
                              {formatMetric(av, mode)}
                            </span>
                          </div>
                        );
                      })}
                      <div className="mt-0.5 flex items-center justify-between gap-1 border-t border-border/50 pt-0.5 text-[10px] font-semibold leading-tight">
                        <span className="text-muted-foreground">∑</span>
                        <span className="tabular-nums">{formatMetric(value, mode)}</span>
                      </div>
                    </div>
                  )}
                </button>
              );
            })}
          </div>
        </CardContent>
      </Card>

      {/* 선택일 상세 모달 */}
      <Dialog
        open={!!selectedDate}
        onOpenChange={(open) => {
          if (!open) setSelectedDate(null);
        }}
      >
        <DialogContent className="flex max-h-[85vh] flex-col gap-4 sm:max-w-2xl">
          {/* 헤더는 스크롤되지 않도록 고정 (닫기 버튼 항상 노출) */}
          <DialogHeader className="shrink-0">
            <DialogTitle className="flex items-center gap-2">
              <CalendarDays className="h-4 w-4 text-muted-foreground" />
              {selectedDate ? dateLabel(selectedDate) : ""} 사용 상세
            </DialogTitle>
            <DialogDescription>
              {mode === "tokens" ? "토큰" : "비용"} 기준 에이전트·플러그인·도구별 사용량 (비용은 도구 호출 기준 추정
              배분)
            </DialogDescription>
            {/* 모달 안에서도 토큰/비용 전환 가능 (뒤의 헤더 토글이 가려지므로) */}
            <div className="pt-1">
              <ModeToggle mode={mode} setMode={setMode} size="sm" />
            </div>
          </DialogHeader>

          {/* 본문만 스크롤 */}
          <div className="flex min-h-0 flex-1 flex-col overflow-y-auto pr-1">
            {selectedRec && selectedRec.total_tokens + selectedRec.total_cost > 0 ? (
              <div className="flex flex-col gap-6">
                {/* 총계 (토글에 맞춰 토큰/비용 표시) */}
                <div className="flex flex-wrap items-center gap-x-10 gap-y-2 rounded-lg border border-border bg-muted/30 p-4">
                  <div className="flex flex-col">
                    <span className="text-xs text-muted-foreground">
                      {mode === "tokens" ? "총 토큰" : "총 비용"}
                    </span>
                    <span
                      className={cn(
                        "text-lg font-semibold tabular-nums",
                        mode === "cost" && "text-primary"
                      )}
                    >
                      {formatMetric(metricOf(selectedRec, mode), mode)}
                    </span>
                  </div>
                </div>

                {/* 에이전트별 사용량 (토글에 맞춰 토큰/비용 표시) */}
                <div className="flex flex-col gap-3">
                  <h4 className="text-sm font-semibold">
                    에이전트별 {mode === "tokens" ? "토큰" : "비용"}
                  </h4>
                  {AGENTS.map((agent) => {
                    const agentVal = (mode === "tokens"
                      ? selectedRec[agent.tokenKey]
                      : selectedRec[agent.costKey]) as number;
                    const dayTotal = metricOf(selectedRec, mode);
                    const pct = dayTotal > 0 ? (agentVal / dayTotal) * 100 : 0;

                    return (
                      <div key={agent.label} className="flex flex-col gap-1.5">
                        <div className="flex items-center justify-between text-sm">
                          <span className={cn("font-medium", agent.text)}>{agent.label}</span>
                          <span
                            className={cn(
                              "tabular-nums",
                              mode === "cost" ? "text-primary" : "text-foreground"
                            )}
                          >
                            {formatMetric(agentVal, mode)}
                          </span>
                        </div>
                        <div className="h-1.5 overflow-hidden rounded-full bg-muted">
                          <div
                            className={cn("h-full rounded-full", agent.bar)}
                            style={{ width: `${pct}%` }}
                          />
                        </div>
                      </div>
                    );
                  })}
                </div>

                {/* 플러그인별 / 도구별 랭킹 (토글에 맞춰 토큰/비용 기준 정렬·표시) */}
                <div className="grid grid-cols-1 gap-6 border-t border-border pt-5 md:grid-cols-2">
                  <RankSection
                    title={`플러그인별 ${mode === "tokens" ? "토큰" : "비용"}`}
                    icon={Puzzle}
                    items={dayBreakdown?.plugins}
                    loading={breakdownLoading}
                    mode={mode}
                  />
                  <RankSection
                    title={`도구별 ${mode === "tokens" ? "토큰" : "비용"}`}
                    icon={Wrench}
                    items={dayBreakdown?.tools}
                    loading={breakdownLoading}
                    mode={mode}
                  />
                </div>
              </div>
            ) : (
              <div className="py-8 text-center text-sm text-muted-foreground">
                이 날짜의 사용 기록이 없습니다.
              </div>
            )}
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

export default CalendarView;
