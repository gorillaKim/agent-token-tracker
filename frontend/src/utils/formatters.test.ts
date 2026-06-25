import { describe, test, expect, beforeEach, afterEach, vi } from "vitest";
import { formatCwd, formatTokens, formatUsd, formatResetTime } from "./formatters";

describe("formatCwd (작업 경로 포맷터) 테스트", () => {
  test("빈 문자열을 전달하면 하이픈(-)을 반환해야 합니다.", () => {
    expect(formatCwd("")).toBe("-");
  });

  test("Unknown 경로가 전달되면 알 수 없음 메시지를 반환해야 합니다.", () => {
    expect(formatCwd("/Unknown")).toBe("(알 수 없음)");
  });

  test("익명화 테스트 프로젝트 경로가 전달되면 테스트 프로젝트 메시지를 반환해야 합니다.", () => {
    expect(formatCwd("/anon/project")).toBe("(테스트 프로젝트)");
  });

  test("임시 경로(/tmp, /var/folders 등)가 전달되면 임시 작업 경로 메시지를 반환해야 합니다.", () => {
    expect(formatCwd("/tmp")).toBe("(임시 작업 경로)");
    expect(formatCwd("/tmp/subfolder")).toBe("(임시 작업 경로)");
    expect(formatCwd("/var/folders/aa/bb")).toBe("(임시 작업 경로)");
    expect(formatCwd("/private/var/folders/xx/yy")).toBe("(임시 작업 경로)");
  });

  test("일반적인 작업 경로가 전달되면 원본 경로를 그대로 반환해야 합니다.", () => {
    const normalPath = "/Users/madup/gorillaProject/agent-token-tracker";
    expect(formatCwd(normalPath)).toBe(normalPath);
  });
});

describe("formatTokens (토큰 단위 변환 포맷터) 테스트", () => {
  test("1,000 미만의 숫자는 그대로 문자열로 반환해야 합니다.", () => {
    expect(formatTokens(0)).toBe("0");
    expect(formatTokens(456)).toBe("456");
    expect(formatTokens(999)).toBe("999");
  });

  test("1,000 이상 1,000,000 미만의 숫자는 K 단위를 소수점 첫째 자리까지 표기해야 합니다.", () => {
    expect(formatTokens(1000)).toBe("1.0K");
    expect(formatTokens(1500)).toBe("1.5K");
    expect(formatTokens(12345)).toBe("12.3K");
    expect(formatTokens(999900)).toBe("999.9K");
  });

  test("1,000,000 이상의 숫자는 M 단위를 소수점 첫째 자리까지 표기해야 합니다.", () => {
    expect(formatTokens(1000000)).toBe("1.0M");
    expect(formatTokens(2400000)).toBe("2.4M");
    expect(formatTokens(12345678)).toBe("12.3M");
  });
});

describe("formatUsd (비용 포맷터) 테스트", () => {
  test("값이 undefined이거나 null인 경우 '0.0'을 반환해야 합니다.", () => {
    expect(formatUsd(undefined)).toBe("0.0");
    expect(formatUsd(null)).toBe("0.0");
  });

  test("전달된 소수점 비용 값을 반올림하여 소수점 첫째 자리까지 문자열로 반환해야 합니다.", () => {
    expect(formatUsd(0)).toBe("0.0");
    expect(formatUsd(0.0034)).toBe("0.0");
    expect(formatUsd(12.345)).toBe("12.3");
    expect(formatUsd(99.99)).toBe("100.0");
  });
});

describe("formatResetTime (시간 리셋 잔여 시간 포맷터) 테스트", () => {
  beforeEach(() => {
    // 2026-06-25 12:00:00 KST (현 시각 고정)
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-25T12:00:00Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  test("resetAtStr 값이 누락된 경우 빈 문자열을 반환해야 합니다.", () => {
    expect(formatResetTime(null)).toBe("");
    expect(formatResetTime(undefined)).toBe("");
    expect(formatResetTime("")).toBe("");
  });

  test("이미 지난 과거의 시간이 전달된 경우 '곧 초기화됨'을 반환해야 합니다.", () => {
    const pastTime = "2026-06-25T11:59:00Z";
    expect(formatResetTime(pastTime)).toBe("곧 초기화됨");
  });

  test("미래의 시간이 전달된 경우 남은 일(d), 시간(h), 분(m)을 포맷팅하여 반환해야 합니다.", () => {
    // 1. 45분 후 초기화 케이스
    const fortyFiveMinsLater = "2026-06-25T12:45:00Z";
    expect(formatResetTime(fortyFiveMinsLater)).toBe("45m 후 초기화");

    // 2. 2시간 15분 후 초기화 케이스
    const twoHoursFifteenMinsLater = "2026-06-25T14:15:00Z";
    expect(formatResetTime(twoHoursFifteenMinsLater)).toBe("2h 15m 후 초기화");

    // 3. 3일 5시간 10분 후 초기화 케이스
    const threeDaysLater = "2026-06-28T17:10:00Z";
    expect(formatResetTime(threeDaysLater)).toBe("3d 5h 10m 후 초기화");
  });
});
