-- 1. 정상 세션 세팅
INSERT OR IGNORE INTO sessions (session_id, agent_type, agent_version, started_at, ended_at, cwd, model_id, total_input_tokens, total_output_tokens, token_source)
VALUES ('sess-normal-1', 'claude_code', '0.1.0', '2026-06-23T10:00:00Z', '2026-06-23T10:02:00Z', '/workspace', 'claude-3-5-sonnet', 500, 200, 'api');

-- 2. 연속 실패 세션 세팅 (repeated_failure)
INSERT OR IGNORE INTO sessions (session_id, agent_type, agent_version, started_at, ended_at, cwd, model_id, total_input_tokens, total_output_tokens, token_source)
VALUES ('sess-fail-loop', 'claude_code', '0.1.0', '2026-06-23T10:05:00Z', NULL, '/workspace', 'claude-3-5-sonnet', 1000, 300, 'api');

INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
VALUES ('sess-fail-loop', 'run_command', '{"cmd":"ls"}', 'hash-f1', 0, 0, '2026-06-23T10:05:10Z');
INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
VALUES ('sess-fail-loop', 'run_command', '{"cmd":"ls"}', 'hash-f2', 0, 0, '2026-06-23T10:05:20Z');
INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
VALUES ('sess-fail-loop', 'run_command', '{"cmd":"ls"}', 'hash-f3', 0, 0, '2026-06-23T10:05:30Z');

-- 3. 동일 호출 반복 세션 세팅 (repeated_call)
INSERT OR IGNORE INTO sessions (session_id, agent_type, agent_version, started_at, ended_at, cwd, model_id, total_input_tokens, total_output_tokens, token_source)
VALUES ('sess-repeat-loop', 'claude_code', '0.1.0', '2026-06-23T10:10:00Z', NULL, '/workspace', 'claude-3-5-sonnet', 800, 150, 'api');

INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
VALUES ('sess-repeat-loop', 'view_file', '{"path":"main.rs"}', 'repeat-hash', 1, 0, '2026-06-23T10:10:10Z');
INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
VALUES ('sess-repeat-loop', 'view_file', '{"path":"main.rs"}', 'repeat-hash', 1, 0, '2026-06-23T10:10:20Z');
INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
VALUES ('sess-repeat-loop', 'view_file', '{"path":"main.rs"}', 'repeat-hash', 1, 0, '2026-06-23T10:10:30Z');

-- 4. 핑퐁 루프 세션 세팅 (ping_pong)
INSERT OR IGNORE INTO sessions (session_id, agent_type, agent_version, started_at, ended_at, cwd, model_id, total_input_tokens, total_output_tokens, token_source)
VALUES ('sess-pingpong-loop', 'claude_code', '0.1.0', '2026-06-23T10:15:00Z', NULL, '/workspace', 'claude-3-5-sonnet', 1200, 400, 'api');

INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
VALUES ('sess-pingpong-loop', 'view_file', '{"path":"A"}', 'h1', 1, 0, '2026-06-23T10:15:10Z');
INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
VALUES ('sess-pingpong-loop', 'run_command', '{"cmd":"B"}', 'h2', 1, 0, '2026-06-23T10:15:20Z');
INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
VALUES ('sess-pingpong-loop', 'view_file', '{"path":"A"}', 'h1', 1, 0, '2026-06-23T10:15:30Z');
INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
VALUES ('sess-pingpong-loop', 'run_command', '{"cmd":"B"}', 'h2', 1, 0, '2026-06-23T10:15:40Z');
INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
VALUES ('sess-pingpong-loop', 'view_file', '{"path":"A"}', 'h1', 1, 0, '2026-06-23T10:15:50Z');
INSERT INTO tool_calls (session_id, tool_name, tool_input, input_hash, success, is_loop_suspect, created_at)
VALUES ('sess-pingpong-loop', 'run_command', '{"cmd":"B"}', 'h2', 1, 0, '2026-06-23T10:16:00Z');
