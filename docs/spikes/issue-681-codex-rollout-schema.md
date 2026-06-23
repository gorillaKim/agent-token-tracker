# Issue #681: Codex rollout token_count 스키마 확정

> Engram Issue #681 · E0. 스파이크 & 데이터 소스 확정  
> Spike 수행일: 2026-06-23  
> 에이전트: main@f64faec8-issue681

## 대상 소스

```
~/.codex/sessions/YYYY/MM/DD/rollout-<uuid>.jsonl
```

실측 샘플: `~/.codex/sessions/2026/06/19/rollout-2026-06-19T11-56-04-019eddce-98ba-7613-a843-8609fd24b540.jsonl`

---

## 1. event_msg.payload.type 분포 (1개 세션 기준)

| payload.type | 건수 | 설명 |
|---|---|---|
| `mcp_tool_call_end` | 34 | MCP 도구 호출 완료 |
| `token_count` | 33 | 토큰 누계 스냅샷 |
| `agent_message` | 26 | 에이전트 주석/해설 |
| `task_started` | 4 | 턴(task) 시작 |
| `user_message` | 4 | 사용자 입력 |
| `task_complete` | 4 | 턴 완료 |
| `patch_apply_end` | 1 | 파일 패치 적용 완료 |

최상위 타입 분포:
- `response_item`: 268 (LLM 응답 조각)
- `event_msg`: 106 (이벤트)
- `session_meta`: 4 (세션 메타)
- `turn_context`: 4 (턴 컨텍스트)

---

## 2. token_count 스키마 (확정)

```json
{
  "type": "token_count",
  "info": {
    "total_token_usage": {
      "input_tokens": <int>,          // 세션 누계 input
      "cached_input_tokens": <int>,   // 세션 누계 cache hit
      "output_tokens": <int>,         // 세션 누계 output
      "reasoning_output_tokens": <int>, // reasoning 포함
      "total_tokens": <int>           // input + output 누계
    },
    "last_token_usage": {             // 해당 턴의 델타
      "input_tokens": <int>,
      "cached_input_tokens": <int>,
      "output_tokens": <int>,
      "reasoning_output_tokens": <int>,
      "total_tokens": <int>
    },
    "model_context_window": <int>
  },
  "rate_limits": { ... }              // 무시 가능
}
```

### 누계 vs 델타 판정: **누계(cumulative)**

검증 근거:
- 1st sample: `total_token_usage.total_tokens = 20187`
- 2nd sample: `total_token_usage.total_tokens = 42774`
- → 단조 증가 → **세션 내 누계**
- `last_token_usage` = 해당 응답 turn의 델타 (turn별 비용 계산에 사용)

### 추출 전략

어댑터에서 **각 turn의 마지막 `token_count` 이벤트의 `last_token_usage`** 를 사용하거나,
세션 전체는 **마지막 `token_count`의 `total_token_usage`** 사용.

---

## 3. mcp_tool_call_end 스키마

```json
{
  "type": "mcp_tool_call_end",
  "call_id": "<str>",
  "invocation": {
    "server": "<mcp_server_name>",
    "tool": "<tool_name>",
    "arguments": { ... }
  },
  "duration": { "secs": <int>, "nanos": <int> },
  "result": {
    "Ok": { "content": [{ "type": "text", "text": "<str>" }] }
    // 또는 "Err": { ... }
  }
}
```

### 매핑 결정
- `invocation.server` + `invocation.tool` → `tool_calls.tool_name`
- `invocation.arguments` → JSON 직렬화 → `tool_calls.tool_input` → `input_hash` 산출
- `result.Ok` 여부 → `tool_calls.success`
- `duration` → 통계용 (필수 아님)

---

## 4. patch_apply_end 스키마

```json
{
  "type": "patch_apply_end",
  "call_id": "<str>",
  "turn_id": "<str>",
  "stdout": "<str>",     // 변경 파일 목록
  "stderr": "<str>",
  "success": <bool>,
  "changes": {
    "<abs_path>": {
      "type": "add" | "modify" | "delete",
      "content": "<str>"  // 신규 파일 내용 (add 시)
    }
  }
}
```

---

## 5. task_started / task_complete 스키마

```json
// task_started
{
  "type": "task_started",
  "turn_id": "<uuid>",
  "started_at": <unix_ts>,
  "model_context_window": <int>,
  "collaboration_mode_kind": "default" | "plan" | ...
}

// task_complete
{
  "type": "task_complete",
  "turn_id": "<uuid>",
  "last_agent_message": "<str>"  // 에이전트 최종 응답 요약
}
```

---

## 6. 정규화 모델 매핑 (확정)

| Codex 필드 | ATK 정규화 필드 | 비고 |
|---|---|---|
| `session_meta.payload.id` | `sessions.session_id` | uuid |
| `session_meta.payload.timestamp` | `sessions.started_at` | ISO8601 |
| `session_meta.payload.cwd` | `sessions.cwd` | 절대경로 |
| `session_meta.payload.model_provider` | `sessions.agent_type` | `"codex"` |
| `session_meta.payload.cli_version` | `sessions.agent_version` | |
| `turn_context.payload.model` | `sessions.model_id` | |
| `token_count.info.last_token_usage.input_tokens` | `messages.input_tokens` | turn 단위 |
| `token_count.info.last_token_usage.cached_input_tokens` | `messages.cache_read_input_tokens` | |
| `token_count.info.last_token_usage.output_tokens` | `messages.output_tokens` | |
| `mcp_tool_call_end.invocation.server+tool` | `tool_calls.tool_name` | |
| `mcp_tool_call_end.invocation.arguments` | `tool_calls.tool_input` (JSON) | input_hash 기반 |
| `token_count.info.total_token_usage.*` | `sessions.total_*_tokens` | 세션 마지막 값 |

`token_source = "api"` (Codex는 실측 토큰 제공)

---

## 7. 완료 조건 점검

- [x] rollout-*.jsonl payload 타입 분포 실측 완료
- [x] token_count 누계 vs 델타 검증 완료 → **누계**, last_usage=델타
- [x] mcp_tool_call_end / patch_apply_end / task_* 필드 매핑 확정
- [x] 정규화 모델 매핑 테이블 작성 완료
