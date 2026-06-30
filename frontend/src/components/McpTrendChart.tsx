import { useState } from "react";
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";
import { cn } from "@/lib/utils";
import { useMcpUsageTrend } from "@/hooks/queries/useDbQueries";
import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  ChartLegend,
  ChartLegendContent,
} from "@/components/ui/chart";

const chartConfig = {
  engram: { label: "Engram (이슈/태스크)", color: "hsl(var(--primary))" },
  doxus: { label: "Doxus (문서 관리)", color: "hsl(200 95% 42%)" },
  playwright: { label: "Playwright (브라우저)", color: "hsl(35 95% 55%)" },
  other: { label: "기타 MCP 도구", color: "hsl(var(--muted-foreground))" },
} satisfies ChartConfig;

const SEG = "rounded-md px-3 py-1 text-sm font-medium transition-colors";
const SEG_ACTIVE = "bg-background text-foreground shadow-sm";
const SEG_IDLE = "text-muted-foreground hover:text-foreground";

export function McpTrendChart() {
  const [days, setDays] = useState<number>(7);
  const { data = [], isLoading } = useMcpUsageTrend(days);

  const formattedData = data.map((item) => {
    let label = item.label;
    if (days === 1) {
      label = `${item.label}:00`;
    } else if (typeof label === "string" && label.length > 5) {
      label = label.substring(5); // "2026-06-30" -> "06-30"
    }
    return {
      label,
      engram: item.engram_calls || 0,
      doxus: item.doxus_calls || 0,
      playwright: item.playwright_calls || 0,
      other: item.other_calls || 0,
    };
  });

  return (
    <section className="rounded-xl border border-border bg-card p-5">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-base font-semibold">
            {days === 1 ? "시간대별 MCP 호출 추이" : "일자별 MCP 호출 추이"}
          </h3>
          <span className="text-sm text-muted-foreground">종류별 MCP(engram/doxus/playwright) 호출 누적 트렌드</span>
        </div>

        <div className="flex items-center gap-0.5 rounded-lg border border-border bg-muted/40 p-0.5">
          {[
            { label: "1d", value: 1 },
            { label: "3d", value: 3 },
            { label: "7d", value: 7 },
          ].map((opt) => (
            <button
              key={opt.value}
              onClick={() => setDays(opt.value)}
              className={cn(
                SEG,
                days === opt.value ? SEG_ACTIVE : SEG_IDLE
              )}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      {isLoading ? (
        <div className="flex h-[280px] w-full items-center justify-center text-sm text-muted-foreground">
          로딩 중...
        </div>
      ) : formattedData.length > 0 ? (
        <ChartContainer config={chartConfig} className="aspect-auto h-[280px] w-full">
          <BarChart data={formattedData} margin={{ left: 4, right: 12, top: 8, bottom: 0 }}>
            <CartesianGrid vertical={false} />
            <XAxis dataKey="label" tickLine={false} axisLine={false} tickMargin={8} minTickGap={24} />
            <YAxis
              tickLine={false}
              axisLine={false}
              width={35}
              domain={[0, "auto"]}
              tickFormatter={(v) => `${v}회`}
            />
            <ChartTooltip
              cursor={{ fill: "hsl(var(--muted)/0.15)" }}
              content={
                <ChartTooltipContent
                  indicator="dashed"
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
                        {value}회
                      </span>
                    </div>
                  )}
                />
              }
            />
            <Bar dataKey="engram" name="engram" stackId="a" fill="var(--color-engram)" radius={[0, 0, 0, 0]} />
            <Bar dataKey="doxus" name="doxus" stackId="a" fill="var(--color-doxus)" radius={[0, 0, 0, 0]} />
            <Bar dataKey="playwright" name="playwright" stackId="a" fill="var(--color-playwright)" radius={[0, 0, 0, 0]} />
            <Bar dataKey="other" name="other" stackId="a" fill="var(--color-other)" radius={[4, 4, 0, 0]} />
            <ChartLegend content={<ChartLegendContent />} className="mt-4" />
          </BarChart>
        </ChartContainer>
      ) : (
        <div className="flex h-[280px] w-full items-center justify-center text-sm text-muted-foreground">
          이 기간 동안 수집된 MCP 호출 이력이 없습니다.
        </div>
      )}
    </section>
  );
}
