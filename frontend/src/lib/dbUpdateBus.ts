import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

/**
 * `db-updated` 이벤트 디바운스 버스 (모듈 레벨 싱글톤)
 *
 * 백엔드는 로그 파일이 자주 바뀌면 `db-updated` 이벤트를 빠르게 연속 emit 한다.
 * 화면마다 개별 리스너가 매 이벤트마다 전체 refetch(외부 API 포함)를 돌리면 앱이 멈춘다.
 *
 * 이 버스는 앱 전체에서 단 하나의 tauri 리스너만 등록하고,
 * 들어오는 이벤트를 ~500ms 디바운스한 뒤 현재 구독자 전원에게 한 번만 알린다.
 *
 * 추가로 "보는 중 동결(freeze-while-viewing)" 게이팅을 한다:
 * 이 창(트레이 팝오버 또는 메인 창)이 사용자에게 보이고 포커스된 동안에는 데이터 구독자를
 * 호출하지 않고 dirty 플래그만 세운다(화면이 흔들리지 않음). 창이 숨겨지거나 포커스를 잃으면
 * 쌓인 변경을 1회 flush 하고, 다시 보이면 1회 최신화한다. 백엔드 수집/DB는 그대로 도므로
 * 데이터는 항상 최신이며, "표시 여부"는 각 창의 webview가 자기 자신만 판단한다.
 *
 * 사용 예:
 *   useEffect(() => dbUpdateBus.subscribe(load), [load]);
 *   const { dirty, refresh } = useDbDirty(); // "새 데이터" 인디케이터
 */

// 디바운스 대기 시간(ms). 이 시간 안에 추가 이벤트가 오면 타이머를 리셋한다.
const DEBOUNCE_MS = 500;

const subscribers = new Set<() => void>();
const dirtySubscribers = new Set<(dirty: boolean) => void>();

let debounceTimer: ReturnType<typeof setTimeout> | null = null;
// 하부 tauri 리스너는 최초 구독 시 단 한 번만 설정한다(StrictMode/리마운트 중복 방지).
let listenPromise: Promise<UnlistenFn> | null = null;

// === 가시성 게이팅 상태 ===
// 이 창이 지금 사용자에게 보이고 포커스됐는가. 상태를 파악하기 전(시드 전)에는
// 비활성(false)으로 두어 섣불리 동결하지 않는다(= 기존 동작과 동일하게 통과).
let isViewActive = false;
// 보는 중에 새 변경이 들어왔는가(동결되어 화면에 아직 반영되지 않음).
let isDirty = false;

let lastFocus = false; // onFocusChanged / isFocused() 시드 결과
let pageVisible = true; // document.visibilityState
let visibilityInit = false;

// Tauri 런타임 여부. 일반 브라우저(dev 목/시각 검증)에는 창·이벤트 API가 없으므로
// 창 포커스 게이팅과 tauri 리스너 등록을 건너뛴다(앱 크래시 방지).
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function notifySubscribers() {
  // 알림 도중 구독 해제가 일어나도 안전하도록 복사본을 순회한다.
  for (const cb of [...subscribers]) {
    cb();
  }
}

function setDirty(value: boolean) {
  if (isDirty === value) return;
  isDirty = value;
  for (const cb of [...dirtySubscribers]) {
    cb(value);
  }
}

function scheduleNotify() {
  if (debounceTimer) {
    clearTimeout(debounceTimer);
  }
  debounceTimer = setTimeout(() => {
    debounceTimer = null;
    // 어떤 상태(포커스 중/비포커스/숨김)든 db-updated 만으로는 즉시 refetch 하지 않고 dirty 로 미룬다.
    // 숨김/비포커스 상태의 두 창(메인+트레이)이 와치독 db-updated 폭주마다 백그라운드 refetch 를
    // 돌려 백엔드와 경합하던 문제를 제거한다. 실제 refetch 는 창이 (다시) 포커스될 때 applyActive 에서
    // 1회만 flush 하거나, 사용자가 새로고침을 누를 때(refreshNow) 수행한다.
    setDirty(true);
  }, DEBOUNCE_MS);
}

function applyActive() {
  const next = lastFocus && pageVisible;
  if (next === isViewActive) return;
  const wasActive = isViewActive;
  isViewActive = next;

  // 창이 (다시) 보이고 포커스되는 순간, 자리비움 동안 쌓인 변경이 있을 때만(dirty) 1회 flush refetch.
  // - blur/hide(active→inactive) 시에는 아무 동작도 하지 않고 dirty 를 유지한다(아무도 안 보는데 refetch 불필요;
  //   다음 show 때 어차피 flush).
  // - 변경이 없으면(!isDirty) 캐시가 이미 최신이므로 show 해도 불필요한 refetch 를 하지 않는다.
  if (!wasActive && next && isDirty) {
    setDirty(false);
    notifySubscribers();
  }
}

function ensureVisibilityListeners() {
  if (visibilityInit) return;
  visibilityInit = true;

  // document 가시성은 브라우저에서도 동작하므로 항상 배선한다.
  if (typeof document !== "undefined") {
    pageVisible = document.visibilityState === "visible";
    document.addEventListener("visibilitychange", () => {
      pageVisible = document.visibilityState === "visible";
      applyActive();
    });
  }

  // 창 포커스 추적은 Tauri 런타임에서만 가능. 브라우저 dev 에서는 게이팅 비활성
  // (lastFocus=false 유지 → 동결 없이 통과).
  if (!isTauri) return;

  const win = getCurrentWindow();
  // 포커스 변화(트레이 팝오버 blur 자동 hide / 메인 창 focus·blur)를 추적.
  void win.onFocusChanged(({ payload: focused }) => {
    lastFocus = focused;
    applyActive();
  });
  // 초기 포커스 상태 시드(리스너 등록 시점에 이미 포커스된 경우 대비).
  void win
    .isFocused()
    .then((focused) => {
      lastFocus = focused;
      applyActive();
    })
    .catch(() => {});
}

function ensureListener() {
  if (listenPromise) return;
  ensureVisibilityListeners();
  // 브라우저 dev(비 Tauri): db-updated 이벤트 소스가 없으므로 리스너 등록 생략.
  if (!isTauri) {
    listenPromise = Promise.resolve(() => {});
    return;
  }
  // 최초 1회만 tauri 리스너 등록. 구독자가 0이 되어도 리스너는 유지한다(단순/안전).
  listenPromise = listen("db-updated", () => {
    scheduleNotify();
  });
}

/**
 * `db-updated` 디바운스 알림을 구독한다.
 * @param callback 디바운스된 이벤트 발생 시 호출되는 콜백
 * @returns 구독 해제 함수 (useEffect cleanup 에서 호출)
 */
function subscribe(callback: () => void): () => void {
  ensureListener();
  subscribers.add(callback);
  return () => {
    subscribers.delete(callback);
  };
}

/**
 * dirty(보는 중 동결되어 미반영된 새 변경 존재) 상태 변화를 구독한다.
 * "새 데이터 있음 / 새로고침" 인디케이터용. 보통 {@link useDbDirty} 훅으로 쓴다.
 */
function subscribeDirty(callback: (dirty: boolean) => void): () => void {
  ensureListener();
  dirtySubscribers.add(callback);
  return () => {
    dirtySubscribers.delete(callback);
  };
}

/** 현재 dirty 여부(동결 중 미반영 변경 존재). */
function getDirty(): boolean {
  return isDirty;
}

/** 사용자 요청에 의한 즉시 flush: dirty 해제 + 구독자 전원 갱신. */
function refreshNow() {
  setDirty(false);
  notifySubscribers();
}

export const dbUpdateBus = {
  subscribe,
  subscribeDirty,
  getDirty,
  refreshNow,
};

/**
 * dirty 상태와 수동 새로고침 함수를 노출하는 React 훅.
 * 동결 중 stale 데이터를 보완하기 위한 "새 데이터 있음 / 새로고침" UI에 사용한다.
 */
export function useDbDirty(): { dirty: boolean; refresh: () => void } {
  const [dirty, setDirtyState] = useState<boolean>(() => getDirty());
  useEffect(() => {
    // 마운트와 구독 사이의 변화 누락 방지를 위해 현재 값으로 재동기화.
    setDirtyState(getDirty());
    return subscribeDirty(setDirtyState);
  }, []);
  return { dirty, refresh: refreshNow };
}
