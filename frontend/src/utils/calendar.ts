/**
 * 캘린더 뷰용 날짜 유틸리티 (외부 의존성 없음)
 *
 * date-fns/dayjs 등 라이브러리를 추가하지 않고 네이티브 Date + Intl 로만 구현합니다.
 * 백엔드가 사용자 PC 로컬 타임존 기준 "YYYY-MM-DD" 문자열을 반환하므로, 프론트도 로컬
 * 타임존 기준 키 형식으로 매칭합니다.
 */

/** 캘린더 그리드의 한 칸(날짜 셀) */
export interface CalendarCell {
  /** "YYYY-MM-DD" — 백엔드 일자 키와 매칭되는 값 */
  date: string;
  /** 일(day of month) 숫자 */
  day: number;
  /** 현재 보고 있는 달에 속하는 날짜인지 (false면 앞/뒤 채움 셀) */
  inMonth: boolean;
}

/** 로컬 Date → "YYYY-MM-DD" (로컬 getter 일관 사용으로 TZ drift 방지) */
function toDateKey(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/**
 * 해당 월(year, month=0~11)의 캘린더 그리드를 생성한다.
 * 1일이 속한 주의 일요일부터 시작해 6주(42칸) 고정 그리드를 반환한다.
 */
export function getMonthGrid(year: number, month: number): CalendarCell[] {
  const firstOfMonth = new Date(year, month, 1);
  // 그리드 시작: 1일이 속한 주의 일요일 (getDay(): 0=일 ~ 6=토)
  const start = new Date(year, month, 1 - firstOfMonth.getDay());

  const cells: CalendarCell[] = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(start.getFullYear(), start.getMonth(), start.getDate() + i);
    cells.push({
      date: toDateKey(d),
      day: d.getDate(),
      inMonth: d.getMonth() === month && d.getFullYear() === year,
    });
  }
  return cells;
}

/** 해당 월의 조회 범위(1일 ~ 말일)를 "YYYY-MM-DD" 로 반환 */
export function monthRange(year: number, month: number): { start: string; end: string } {
  const first = new Date(year, month, 1);
  const last = new Date(year, month + 1, 0); // 다음 달 0일 = 이번 달 말일
  return { start: toDateKey(first), end: toDateKey(last) };
}

/** 로컬(PC) 타임존 기준 오늘 날짜 키 ("YYYY-MM-DD") — 백엔드 일자와 정렬되어 "오늘" 셀 강조에 사용 */
export function localTodayKey(): string {
  // 로컬 getter 일관 사용으로 그리드/조회 범위와 동일한 타임존 기준 유지
  return toDateKey(new Date());
}

/** 로컬(PC) 타임존 기준 현재 연/월(month=0~11) — 캘린더 초기 표시 월 */
export function localCurrentYearMonth(): { year: number; month: number } {
  const now = new Date();
  return { year: now.getFullYear(), month: now.getMonth() };
}

/** "2026년 6월" 형태의 월 라벨 */
export function monthLabel(year: number, month: number): string {
  return `${year}년 ${month + 1}월`;
}

/** "YYYY-MM-DD" → "6월 25일 (수)" 형태의 한국어 날짜 라벨 */
export function dateLabel(dateKey: string): string {
  const [y, m, d] = dateKey.split("-").map(Number);
  const date = new Date(y, m - 1, d);
  const weekdays = ["일", "월", "화", "수", "목", "금", "토"];
  return `${m}월 ${d}일 (${weekdays[date.getDay()]})`;
}

/** 요일 헤더 라벨 (일~토) */
export const WEEKDAY_LABELS = ["일", "월", "화", "수", "목", "금", "토"] as const;
