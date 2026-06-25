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
 * 에이전트 토큰 리셋 잔여 시간을 사용자가 알아보기 쉽게 남은 일(d), 시간(h), 분(m)으로 변환합니다.
 * 
 * @param resetAtStr ISO8601 형식의 초기화 예정 시간 문자열
 * @returns 잔여 시간 문자열
 */
export const formatResetTime = (resetAtStr: string | null | undefined): string => {
  if (!resetAtStr) return "";
  const diffMs = new Date(resetAtStr).getTime() - Date.now();
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
