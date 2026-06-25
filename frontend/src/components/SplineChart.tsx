import { useState, useMemo } from "react";
import { Area, CartesianGrid, ComposedChart, Line, XAxis, YAxis } from "recharts";
import { formatTokens } from "../utils/formatters";
import { cn } from "@/lib/utils";
import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  ChartLegend,
  ChartLegendContent,
} from "@/components/ui/chart";

interface SplineChartProps {
  dailyTokenUsage: any[];
  hourlyTokenUsage: any[];
}

const chartConfig = {
  claude: { label: "Anthropic (Claude)", color: "hsl(var(--agent-claude))" },
  codex: { label: "OpenAI (Codex)", color: "hsl(var(--agent-codex))" },
  antigravity: { label: "Antigravity (Local)", color: "hsl(var(--agent-antigravity))" },
  total: { label: "전체 합계", color: "hsl(var(--muted-foreground))" },
} satisfies ChartConfig;

const SEG = "rounded-md px-3 py-1 text-sm font-medium transition-colors";
const SEG_ACTIVE = "bg-background text-foreground shadow-sm";
const SEG_IDLE = "text-muted-foreground hover:text-foreground";

/**
 * 일자/시간별 토큰 사용량 추이 차트 (Recharts + shadcn chart 기반)
 *
 * 에이전트별(Claude/Codex/Antigravity) 영역 + 전체 합계 점선 라인을 렌더링하며,
 * Recharts의 안정적인 인터랙티브 툴팁/반응형 컨테이너를 사용한다.
 */
export function SplineChart({ dailyTokenUsage, hourlyTokenUsage }: SplineChartProps) {
  const [chartViewMode, setChartViewMode] = useState<"daily" | "hourly">("daily");
  const [chartDays, setChartDays] = useState<number>(14);

  const chartData = useMemo(() => {
    if (chartViewMode === "daily") {
      return dailyTokenUsage.slice(-chartDays).map((d) => ({
        label: typeof d.date === "string" && d.date.length > 5 ? d.date.substring(5) : d.date,
        claude: d.claude_tokens || 0,
        codex: d.codex_tokens || 0,
        antigravity: d.antigravity_tokens || 0,
        total: d.total_tokens || 0,
      }));
    }
    return hourlyTokenUsage.map((d) => ({
      label: `${d.hour}:00`,
      claude: d.claude_tokens || 0,
      codex: d.codex_tokens || 0,
      antigravity: d.antigravity_tokens || 0,
      total: d.total_tokens || 0,
    }));
  }, [chartViewMode, chartDays, dailyTokenUsage, hourlyTokenUsage]);

  return (
    <section className="rounded-xl border border-border bg-card p-5">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-base font-semibold">
            {chartViewMode === "daily" ? "일자별 토큰 사용량 추이" : "시간대별 토큰 사용량 추이"}
          </h3>
          <span className="text-sm text-muted-foreground">최근 에이전트 토큰 누적 흐름</span>
        </div>

        <div className="flex items-center gap-2">
          {chartViewMode === "daily" && (
            <div className="flex items-center gap-0.5 rounded-lg border border-border bg-muted/40 p-0.5">
              {[
                { label: "3d", value: 3 },
                { label: "7d", value: 7 },
                { label: "14d", value: 14 },
                { label: "1m", value: 30 },
              ].map((opt) => (
                <button
                  key={opt.value}
                  onClick={() => setChartDays(opt.value)}
                  className={cn(
                    "rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                    chartDays === opt.value ? SEG_ACTIVE : SEG_IDLE
                  )}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          )}

          <div className="flex items-center gap-0.5 rounded-lg border border-border bg-muted/40 p-0.5">
            <button
              onClick={() => setChartViewMode("daily")}
              className={cn(SEG, chartViewMode === "daily" ? SEG_ACTIVE : SEG_IDLE)}
            >
              일자별
            </button>
            <button
              onClick={() => setChartViewMode("hourly")}
              className={cn(SEG, chartViewMode === "hourly" ? SEG_ACTIVE : SEG_IDLE)}
            >
              시간대별
            </button>
          </div>
        </div>
      </div>

      {chartData.length > 0 ? (
        <ChartContainer config={chartConfig} className="aspect-auto h-[280px] w-full">
          <ComposedChart data={chartData} margin={{ left: 4, right: 12, top: 8, bottom: 0 }}>
            <defs>
              {(["claude", "codex", "antigravity"] as const).map((k) => (
                <linearGradient key={k} id={`fill-${k}`} x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor={`var(--color-${k})`} stopOpacity={0.2} />
                  <stop offset="95%" stopColor={`var(--color-${k})`} stopOpacity={0} />
                </linearGradient>
              ))}
            </defs>
            <CartesianGrid vertical={false} />
            <XAxis dataKey="label" tickLine={false} axisLine={false} tickMargin={8} minTickGap={24} />
            <YAxis
              tickLine={false}
              axisLine={false}
              width={52}
              domain={[0, "auto"]}
              tickFormatter={(v) => formatTokens(Number(v))}
            />
            <ChartTooltip
              cursor={{ stroke: "hsl(var(--border))", strokeDasharray: "4 4" }}
              content={
                <ChartTooltipContent
                  indicator="dot"
                  formatter={(value, name, item) => (
                    <div className="flex w-full items-center justify-between gap-3">
                      <span className="flex items-center gap-1.5 text-muted-foreground">
                        <span
                          className="h-2 w-2 shrink-0 rounded-[2px]"
                          style={{ backgroundColor: item.color }}
                        />
                        {chartConfig[name as keyof typeof chartConfig]?.label ?? name}
                      </span>
                      <span className="font-mono font-medium tabular-nums text-foreground">
                        {formatTokens(Number(value))}
                      </span>
                    </div>
                  )}
                />
              }
            />
            <Area
              dataKey="claude"
              type="monotone"
              stroke="var(--color-claude)"
              strokeWidth={2}
              fill="url(#fill-claude)"
              dot={false}
              isAnimationActive={false}
            />
            <Area
              dataKey="codex"
              type="monotone"
              stroke="var(--color-codex)"
              strokeWidth={2}
              fill="url(#fill-codex)"
              dot={false}
              isAnimationActive={false}
            />
            <Area
              dataKey="antigravity"
              type="monotone"
              stroke="var(--color-antigravity)"
              strokeWidth={2}
              fill="url(#fill-antigravity)"
              dot={false}
              isAnimationActive={false}
            />
            <Line
              dataKey="total"
              type="monotone"
              stroke="var(--color-total)"
              strokeWidth={1.5}
              strokeDasharray="4 4"
              dot={false}
              isAnimationActive={false}
            />
            <ChartLegend content={<ChartLegendContent />} />
          </ComposedChart>
        </ChartContainer>
      ) : (
        <div className="flex h-[280px] items-center justify-center text-sm text-muted-foreground">
          기간 내 수집된 에이전트 토큰 데이터가 존재하지 않습니다.
        </div>
      )}
    </section>
  );
}
export default SplineChart;
