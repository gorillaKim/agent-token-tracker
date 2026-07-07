# ATK 오작동 탐지 강화 — 구현 지시서 (for ATK 에이전트)

## 배경 / 문제
현재 ATK 탐지는 카운트·tool명 기반 절대 임계값이라 실데이터에서 노이즈는 많고 진짜 오작동은 놓친다. 362세션(claude_code 230 / antigravity 70 / codex 62) 실측 결과:

- dynamic_ping_pong{cycles≥3} → tool-bearing claude_code of 18/149(12.1%) 발화, 사실상 전부 정상 Read/Edit 반복 (input 무시 tool명 매칭의 한계).
- 진짜 exact-repeat(dynamic_repeated_calls run≥3)는 3/149(2%), is_loop_suspect는 22,897콜 중 0 (코드가 아예 안 씀).
- claude_code 실패 기록 = 18,404콜 중 3건 → 실패/지연 규칙군 사실상 죽음.
- unexpected_exit = claude_code 230/230(100%) → 변별력 0 (ended_at 항상 NULL).
- token_inefficiency = 대부분 아티팩트 (codex output_tokens=0 롤업 버그 50/62 + 스텁 세션).
- subagent_anomaly_limit = parent_session_id 0/362 → 영원히 발화 불가.
- 근본: 규칙이 카운트만 보고, 에이전트가 실제로 잘못됐는지(엉뚱한 경로·지시 무시·thrashing·사용자가 정정)는 못 본다.

## 0. 현재 시그널 실태 (테이블이 실제로 담는 것)
- tool_calls.success: claude_code 3실패/18,404 · codex 2/119 · antigravity 0/4,374 (claude_code.rs:244,257, codex.rs:147)
- tool_calls.is_loop_suspect: 0/22,897 — 어떤 경로도 안 씀. detect/loops.rs는 호출자 없는 죽은 코드 (db.rs:309, loops.rs)
- tool_calls.tool_input: 정규화 JSON. Edit file_path 추출 2340/2340=100%. Agent 스폰은 {description,prompt} 보유 (claude_code.rs:206, adapters/mod.rs:33)
- tool 결과 텍스트/에러: result_char_count만 저장, 에러/결과 TEXT는 폐기 (claude_code.rs:245-267)
- tool_name='Agent': 58세션에 362 스폰 — 서브에이전트 호출은 잡힘 (DB)
- messages.content: claude_code user 664개 중 59개가 [request interrupted by user](26세션). codex 0. antigravity 98% 합성 (claude_code.rs:158, antigravity.rs:373)
- messages.role: claude_code 과거 assistant(38,014), 어댑터가 이제 agent로 정규화 → 불일치 라이브 (claude_code.rs:292)
- messages.cost_usd: role=="assistant"일 때만 계산 → 향후 claude_code(agent) 행은 $0 (ingest.rs:219)
- sessions.ended_at: claude_code 230/230 NULL (session_end 미방출) (claude_code.rs:349)
- sessions.parent_session_id: 0/362 — 컬럼만 존재 (claude_code.rs:374)
- codex 토큰: 50/62 세션 output=0 (롤업이 total_tokens→input, output=0) (codex.rs:123-125)

## 5대 핵심 구현 사항
1. **F1 — 사용자 정정/중단 마이닝**: UserInterruptionLimit을 messages.content로 rewire + UserCorrectionSignal
2. **F4 — 데이터 품질 3종**: role 통일 + codex output=0 보정 + is_loop_suspect 사후 백필
3. **F2 — 진전 인식 FileChurn**: tool명 ping-pong 대체, F1과 AND 결합
4. **F3 — CohortPercentileExceeds**: 절대 임계값 노이즈 제거, 동종 세션 대비 이상치 규칙
5. **F6 — export_session_context MCP 도구**: F1/F2 앵커 주변 타임라인을 플러그인 LLM analyst에 공급
