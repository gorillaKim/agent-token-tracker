# ATK 테스트 픽스처 익명화 정책 (이슈 #706)

## 목적
파서·dedup·가격계산의 회귀를 막는 `cargo test` 유닛테스트에 사용할
익명화된 샘플 JSONL 픽스처를 이 디렉토리에 보관합니다.

## 보관 대상
| 파일 | 어댑터 | 설명 |
|------|--------|------|
| `claude_code_basic.jsonl` | ClaudeCode | 기본 session_meta + user/assistant 메시지 + tool_use |
| `claude_code_thinking.jsonl` | ClaudeCode | thinking 블록 포함 + cache_read 토큰 |
| `claude_code_multi_turn.jsonl` | ClaudeCode | 다중 턴, 여러 도구 호출, session_end |
| `codex_basic.jsonl` | Codex | session_meta + turn_context + token_count + mcp_tool |
| `codex_patch.jsonl` | Codex | patch_apply_end 노드 포함 |

## 익명화 기준
실제 로그에서 픽스처를 생성할 때 다음 필드를 익명화합니다:

1. **session_id / id**: `anon-sess-<N>` 형식으로 대체
2. **cwd**: `/anon/project` 로 대체
3. **user 메시지 content**: `"익명화된 사용자 입력 <N>"` 으로 대체
4. **assistant content text**: `"익명화된 AI 응답 <N>"` 으로 대체
5. **tool_input의 file paths**: `/anon/path/file.txt` 로 대체
6. **thinking 내용**: `"익명화된 사고과정 <N>"` 으로 대체
7. **timestamp**: `2026-01-01T00:0N:00Z` 형식의 순차 시각으로 대체

## 보관 규칙
- 픽스처 파일은 `tests/fixtures/` 아래에만 커밋합니다.
- 실제 사용자 데이터(~/.claude/projects, ~/.codex/ 등)는 절대 커밋하지 않습니다.
- `.gitignore`에 `~/.claude` 경로는 이미 제외되어 있습니다.
- 픽스처 파일 자체는 레포지토리에 포함되어 CI에서 재현 가능해야 합니다.
