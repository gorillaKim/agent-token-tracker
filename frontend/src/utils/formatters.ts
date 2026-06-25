/**
 * 프론트엔드 UI 포맷팅 유틸리티 함수 모듈
 * 
 * TDD 개발 방법론에 따라 작성되었으며, 순수 함수로 구성되어 테스트가 쉽습니다.
 * 사용자의 한국어 문서화 선호에 맞춰 주석 및 문서화가 한국어로 작성되었습니다.
 */

/**
 * 작업 경로(cwd)를 사용자 친화적인 설명으로 포맷팅합니다.
 * 
 * @param cwd 원본 경로 문자열
 * @returns 가독성이 보완된 경로 또는 설명
 */
export const formatCwd = (cwd: string): string => {
  if (!cwd) return "-";
  if (cwd === "/Unknown") return "(알 수 없음)";
  if (cwd === "/anon/project") return "(테스트 프로젝트)";
  if (
    cwd.startsWith("/private/var/folders/") || 
    cwd.startsWith("/var/folders/") || 
    cwd.startsWith("/tmp/") ||
    cwd === "/tmp"
  ) {
    return "(임시 작업 경로)";
  }
  return cwd;
};

/**
 * 토큰 수를 K(천), M(백만) 단위의 축약형으로 포맷팅합니다.
 * 
 * @param val 토큰 개수
 * @returns 축약된 토큰 개수 문자열 (예: 1.5M, 20.3K, 350)
 */
export const formatTokens = (val: number): string => {
  if (val >= 1_000_000) return `${(val / 1_000_000).toFixed(1)}M`;
  if (val >= 1_000) return `${(val / 1_000).toFixed(1)}K`;
  return val.toString();
};

/**
 * 비용(USD)을 소수점 첫째 자리까지 표기하도록 포맷팅합니다.
 * 
 * @param val USD 비용 값
 * @returns 소수점 1자리 문자열 (예: 12.5)
 */
export const formatUsd = (val: number | undefined | null): string => {
  if (val === undefined || val === null) return "0.0";
  return val.toFixed(1);
};

/**
 * 백엔드가 전달하는 시각 문자열을 Date 객체로 안전하게 파싱합니다.
 *
 * 백엔드는 모든 시각을 **UTC 기준**으로 저장/전달하지만, 출처에 따라 포맷이 섞여 있습니다.
 *  - 세션/메시지 로그 타임스탬프 : "2026-06-25T09:12:03Z"   (ISO8601 + Z)
 *  - SQLite datetime() 산출값(리셋 시각 등) : "2026-06-25 13:00:00" (공백 구분, TZ 표기 없음)
 *  - 외부 API(Anthropic) resets_at : "2026-06-25T13:00:00+00:00" 등 오프셋 포함
 *
 * 타임존 표기가 없는 값은 UTC 로 간주하여 'Z' 를 붙입니다. 그대로 두면 V8 이 공백 구분
 * 문자열을 **로컬 시각으로 오인**해 타임존 오프셋만큼 어긋나기 때문입니다.
 *
 * @param raw 백엔드 시각 문자열
 * @returns 파싱된 Date (UTC 기준 절대 시각). 파싱 불가 시 Invalid Date.
 */
export const parseServerDate = (raw: string): Date => {
  // 공백 구분(SQLite) → ISO 'T' 구분으로 정규화
  const s = raw.trim().replace(" ", "T");
  // 시각 성분이 있고(타임존 표기가 가능하고) Z/±HH:MM 가 없으면 UTC 로 간주
  if (s.includes("T") && !/([zZ]|[+-]\d{2}:?\d{2})$/.test(s)) {
    return new Date(`${s}Z`);
  }
  return new Date(s);
};

/**
 * UTC 시각 문자열을 사용자 PC 로컬 타임존의 "HH:MM:SS" 로 표시합니다.
 *
 * @param raw 백엔드 시각 문자열(UTC)
 * @returns 로컬 타임존 기준 24시간제 시:분:초 (예: "18:12:03"). 값이 없거나 파싱 실패 시 "-"
 */
export const formatLocalTime = (raw: string | null | undefined): string => {
  if (!raw) return "-";
  const d = parseServerDate(raw);
  if (isNaN(d.getTime())) return "-";
  return d.toLocaleTimeString("ko-KR", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
};

/**
 * UTC 시각 문자열을 사용자 PC 로컬 타임존의 "YYYY-MM-DD HH:MM:SS" 로 표시합니다.
 *
 * 로케일별 표기 차이를 피하기 위해 로컬 getter 로 직접 고정 포맷을 구성합니다.
 *
 * @param raw 백엔드 시각 문자열(UTC)
 * @returns 로컬 타임존 기준 날짜·시각. 값이 없거나 파싱 실패 시 "-"
 */
export const formatLocalDateTime = (raw: string | null | undefined): string => {
  if (!raw) return "-";
  const d = parseServerDate(raw);
  if (isNaN(d.getTime())) return "-";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(
    d.getHours()
  )}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
};

/**
 * 에이전트 토큰 리셋 잔여 시간을 사용자가 알아보기 쉽게 남은 일(d), 시간(h), 분(m)으로 변환합니다.
 *
 * @param resetAtStr ISO8601 형식의 초기화 예정 시간 문자열(UTC)
 * @returns 잔여 시간 문자열
 */
export const formatResetTime = (resetAtStr: string | null | undefined): string => {
  if (!resetAtStr) return "";
  const diffMs = parseServerDate(resetAtStr).getTime() - Date.now();
  if (diffMs <= 0) return "곧 초기화됨";
  
  const diffMins = Math.ceil(diffMs / 60000);
  const days = Math.floor(diffMins / 1440);
  const hrs = Math.floor((diffMins % 1440) / 60);
  const mins = diffMins % 60;
  
  let result = "";
  if (days > 0) result += `${days}d `;
  if (hrs > 0 || days > 0) result += `${hrs}h `;
  result += `${mins}m 후 초기화`;
  
  return result.trim();
};
