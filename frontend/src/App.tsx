import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Session {
  session_id: string;
  agent_type: string;
  agent_version?: string;
  started_at: string;
  ended_at?: string;
  cwd: string;
  model_id?: string;
  total_input_tokens: number;
  total_output_tokens: number;
}

interface AgentSummary {
  agent_type: string;
  session_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
}

interface LoopDetectionResult {
  session_id: string;
  is_anomaly: boolean;
  signals: any[];
}

function App() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [summaries, setSummaries] = useState<AgentSummary[]>([]);
  const [anomalies, setAnomalies] = useState<LoopDetectionResult[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function loadData() {
      try {
        const sessList = await invoke<Session[]>("get_active_sessions");
        setSessions(sessList);

        const sumList = await invoke<AgentSummary[]>("get_agent_summaries");
        setSummaries(sumList);

        const anomalyList = await invoke<LoopDetectionResult[]>("get_loop_signals");
        setAnomalies(anomalyList);
      } catch (err: any) {
        setError(err.toString());
      }
    }
    loadData();
  }, []);

  return (
    <div style={{ padding: "20px", textAlign: "left" }}>
      <h1>Agent Token Tracker - 데스크톱 IPC 검증</h1>
      {error && <div style={{ color: "red" }}>오류 발생: {error}</div>}

      <section>
        <h2>1. 에이전트 요약 통계 (Agent Summaries)</h2>
        <table border={1} cellPadding={8} style={{ borderCollapse: "collapse", width: "100%" }}>
          <thead>
            <tr>
              <th>Agent Type</th>
              <th>Sessions</th>
              <th>Total Input</th>
              <th>Total Output</th>
              <th>Total Cost (USD)</th>
            </tr>
          </thead>
          <tbody>
            {summaries.map((s) => (
              <tr key={s.agent_type}>
                <td>{s.agent_type}</td>
                <td>{s.session_count}</td>
                <td>{s.total_input_tokens}</td>
                <td>{s.total_output_tokens}</td>
                <td>${s.total_cost_usd.toFixed(6)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section>
        <h2>2. 활성 세션 목록 (Active Sessions)</h2>
        <ul>
          {sessions.map((s) => (
            <li key={s.session_id}>
              [{s.agent_type}] {s.session_id} - {s.cwd} (입력: {s.total_input_tokens}, 출력: {s.total_output_tokens})
            </li>
          ))}
        </ul>
      </section>

      <section>
        <h2>3. 이상 징후 세션 리스트 (Anomalies)</h2>
        <ul>
          {anomalies.map((a) => (
            <li key={a.session_id} style={{ color: "orange" }}>
              이상 탐지 세션: {a.session_id} - 시그널 수: {a.signals.length}개
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}

export default App;
