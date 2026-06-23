# Issue #683: Graceful Degrade 정책 정의

> Engram Issue #683 · E0. 스파이크 & 데이터 소스 확정  
> 작성일: 2026-06-23  
> 기반: Issue #681 (Codex 스파이크), Issue #682 (Antigravity PoC) 결과  
> 에이전트: main@f64faec8-issue683

---

## 배경

실측 소스의 특성:
- **Antigravity**: 토큰 수치 로컬 미저장 (크레딧 SaaS) → 구조적 한계
- **Codex**: rollout JSONL은 안정적이나 버전별 payload 변형 가능
- **Claude Code**: JSONL 대용량(637MB), 일부 파일 손상 가능성
- **공통**: vscdb 잠금, 파일 권한 오류, 미지원 필드, 포맷 변형

전 어댑터가 공유할 **degrade 규약**이 없으면 scan이 불안정해짐.

---

## 정책 1: token_source 표기 규약

모든 세션은 `token_source` 필드를 필수 포함:

| 값 | 의미 | 해당 에이전트 |
|---|---|---|
| `"api"` | 토큰 수치 직접 파싱 성공 | Claude Code, Codex |
| `"unavailable"` | 구조적으로 토큰 미저장 | Antigravity (항상) |
| `"parse_error"` | 파싱 시도 실패 (포맷 변형 등) | 모든 에이전트 |
| `"db_locked"` | DB 파일 잠금 | Codex (state_5.sqlite), Antigravity (vscdb) |
| `"permission_denied"` | 파일 접근 불가 | 모든 에이전트 |

---

## 정책 2: 부분 실패 처리 원칙

### 2-1. 세션 레벨 degrade

```
scan 중 단일 세션 파싱 실패 → 세션 스킵(skip), scan 계속 진행
실패 원인 → token_source 필드에 기록
스킵 카운트 → 최종 report에 노출 (warnings 섹션)
```

scan이 **절대 panic하거나 중단되지 않음**.

### 2-2. 필드 레벨 degrade

```
미지원/미인식 필드 → 조용히 스킵 (log::debug 레벨)
필수 필드(session_id, agent_type) 누락 → 세션 전체 스킵
선택 필드(model_id, git_branch 등) 누락 → None/null로 적재
```

### 2-3. DB 잠금 degrade

```
SQLite busy → PRAGMA busy_timeout=3000 (3초 대기)
3초 후도 잠금 → token_source='db_locked', 세션 스킵
```

### 2-4. Antigravity 전용 정책

```
vscdb 접근 성공 → 활동(step_count, title, 타임스탬프, workspace) 적재
               → token_source='unavailable' (항상)
               → input/output_tokens = 0 (cost_usd = 0.0)
detect/loops 실행 → step_count 기반 이상치 탐지만 (토큰 기반 불가)
```

---

## 정책 3: 로그/카운트 가시화

```rust
// scan 완료 후 ScanResult 구조체
pub struct ScanResult {
    pub sessions_total: usize,
    pub sessions_inserted: usize,
    pub sessions_skipped: usize,
    pub skip_reasons: HashMap<String, usize>,  // token_source → count
    pub warnings: Vec<String>,
}
```

report 커맨드 출력 예시:
```
scanned: 817 files | inserted: 412 sessions | skipped: 5
  skip reasons: parse_error=3, db_locked=1, permission_denied=1
```

---

## 정책 4: 멱등성(idempotency) 보장

```
재스캔 시 동일 세션 → dedupe_hash 충돌 → INSERT OR IGNORE
token_source가 변경된 경우(parse_error → api) → UPDATE 허용
```

---

## 정책 5: 에이전트별 적용 요약

| 에이전트 | token_source | 토큰 | 활동 | 루프탐지 |
|---|---|---|---|---|
| Claude Code | `api` | ✅ 실측 | ✅ | ✅ 토큰+반복 기반 |
| Codex | `api` | ✅ 실측 | ✅ | ✅ 토큰+반복 기반 |
| Antigravity | `unavailable` | ❌ 0 | ✅ step_count | ⚠️ step_count만 |

---

## 완료 조건 점검

- [x] token_source 열거값 5종 확정
- [x] 세션/필드/DB 레벨 degrade 원칙 문서화
- [x] Antigravity 전용 정책 (token_source=unavailable, 활동만 적재)
- [x] vscdb 잠금 대응 정책 (busy_timeout=3000)
- [x] ScanResult 구조 정의 (warnings/skip_reasons 가시화)
- [x] 멱등성 보장 정책 문서화
