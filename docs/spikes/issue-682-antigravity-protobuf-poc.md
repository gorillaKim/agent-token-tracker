# Issue #682: Antigravity protobuf 역공학 PoC

> Engram Issue #682 · E0. 스파이크 & 데이터 소스 확정  
> Spike 수행일: 2026-06-23  
> 에이전트: main@f64faec8-issue682

## 대상 소스

```
~/Library/Application Support/Antigravity/User/globalStorage/state.vscdb
→ ItemTable.key = 'antigravityUnifiedStateSync.trajectorySummaries'
```

---

## 1. 전체 인코딩 파이프라인 (확정)

```
state.vscdb (SQLite)
  └── ItemTable
        └── key='antigravityUnifiedStateSync.trajectorySummaries'
              └── value: Base64 문자열 (length ~291936)
                    └── Base64 디코드 → 바이너리 protobuf (~218952 bytes)
                          └── repeated TrajectorySummary (field 1)
                                ├── field 1: conversation_id (string, UUID)
                                └── field 2: 내부 protobuf
                                      └── field 1: 다시 Base64 문자열
                                            └── Base64 디코드 → TrajectorySummaryDetail protobuf
```

**이중 Base64 + 이중 protobuf 구조** 확인.

---

## 2. TrajectorySummaryDetail 필드 역공학 결과

디코드 후 protobuf 필드 번호별 타입/의미:

| Field # | Wire Type | 타입 | 확정 내용 | 안정성 |
|---|---|---|---|---|
| 1 | string | **title** | 대화 제목 (예: "Mandate Doc-Kit Usage") | ✅ 안정 |
| 2 | varint | **step_count** (추정) | 단계 수 (예: 74) | ✅ 안정 |
| 3 | message | **created_at** (Timestamp) | sf1=unix_ts_sec, sf2=nanos | ✅ 안정 |
| 4 | string | **conversation_id** | UUID (예: "78eaeda7-1478-4bb1-b8be-6c6b526c45a9") | ✅ 안정 |
| 5 | varint | **status** (enum) | 1=완료 추정 | ⚠️ 추정 |
| 7 | message | **started_at** (Timestamp) | sf1=unix_ts_sec, sf2=nanos | ✅ 안정 |
| 9 | message | **workspace_info** | workspace 경로 + git remote URL + branch | ✅ 안정 |
| 10 | message | **updated_at** (Timestamp) | sf1=unix_ts_sec, sf2=nanos | ✅ 안정 |
| 12 | message | **model_info** (추정) | large int bytes (모델 ID 해시 또는 embedding?) | ⚠️ 불명확 |
| 14 | message | **unknown** | large int bytes | ❌ 미확정 |
| 15 | message | **paths_to_review** (추정) | 파일 경로 목록 | ⚠️ 추정 |
| 16 | varint | **model_enum** (추정) | 71 등 숫자 (모델 ID 열거형?) | ⚠️ 추정 |

### field 9 workspace_info 상세 (확정)

```
field 9 raw bytes 디코드 예시:
  \x1e file:///Users/madup/front-core
  \x12 \x1e file:///Users/madup/front-core
  \x1a C
    \x14 madup-inc/front-core
    \x12 + https://github.com/madup-inc/front-core.git
    \"  \t feat/jake
```

→ workspace_root, git_remote_url, git_branch 추출 가능

---

## 3. 안정적으로 복원 가능한 필드 목록 (확정)

| 필드 | 안정성 | 활용 |
|---|---|---|
| `title` (f1) | ✅ 안정 | 대화 제목 |
| `conversation_id` (f4) | ✅ 안정 | sessions.session_id |
| `created_at` (f3) | ✅ 안정 | sessions.started_at |
| `started_at` (f7) | ✅ 안정 | sessions.started_at (정밀) |
| `updated_at` (f10) | ✅ 안정 | sessions.ended_at 근사 |
| `step_count` (f2) | ✅ 안정 | 활동 지표 |
| `workspace_root` (f9 내부) | ✅ 안정 | sessions.cwd |
| `git_remote` (f9 내부) | ✅ 안정 | 프로젝트 매핑 |
| `git_branch` (f9 내부) | ✅ 안정 | 컨텍스트 |
| `model_info` (f12/f16) | ⚠️ 불안정 | 미사용 권장 |

---

## 4. 토큰 가용성 확정

**Antigravity 토큰 수치는 로컬에 저장되지 않음** (크레딧 기반 SaaS).

trajectorySummaries에는 토큰 필드 없음 → 확정.

→ `token_source = 'unavailable'` 로 세션 적재, 활동(step_count)/루프만 추출.

---

## 5. vscdb 동시성 / 잠금 정책

- SQLite WAL 모드 여부: 확인 필요 (read-only 연결로 안전하게 접근)
- 권장: `PRAGMA journal_mode=WAL` + `PRAGMA busy_timeout=5000`
- 잠금 실패 시 → graceful degrade: `token_source='db_locked'` 로 세션 스킵

---

## 6. 어댑터 구현 범위 (확정)

```rust
// src/adapters/antigravity.rs 구현 대상
pub struct AntigravitySession {
    pub conversation_id: String,   // field 4
    pub title: String,             // field 1
    pub step_count: u64,           // field 2
    pub started_at: i64,           // field 7 unix_sec
    pub updated_at: i64,           // field 10 unix_sec
    pub workspace_root: String,    // field 9
    pub git_remote: Option<String>,// field 9
    pub git_branch: Option<String>,// field 9
    pub token_source: TokenSource, // always Unavailable
}
```

---

## 7. 완료 조건 점검

- [x] state.vscdb 접근 및 trajectorySummaries 키 확인 완료
- [x] Base64 디코드 → protobuf 파싱 PoC 성공
- [x] 이중 인코딩 구조(Base64→protobuf→Base64→protobuf) 확정
- [x] 안정 복원 가능 필드 목록 확정 (title/ts/workspace/git/steps)
- [x] 토큰 미저장 확인 → token_source='unavailable' 정책 확정
- [x] vscdb 잠금 대응 전략 수립
