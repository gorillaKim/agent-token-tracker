#!/usr/bin/env bash
# scripts/cross_check.sh — ATK ↔ ccusage / rtk gain 교차검증 하니스 (이슈 #705)
#
# 사용법:
#   ./scripts/cross_check.sh [--since YYYY-MM-DD] [--db atk.db]
#                            [--output-tolerance 5.0] [--cost-tolerance 10.0]
#
# 의존성: cargo(ATK 바이너리), npx(ccusage), rtk(선택)
#
# 종료 코드:
#   0 — 모든 허용오차 통과
#   2 — 허용오차 초과 (ATK crosscheck 리포트 참고)
#   1 — 도구 없음 / 파싱 실패 등 일반 오류

set -euo pipefail

# ── 기본값 ────────────────────────────────────────────────────
SINCE="${CROSS_CHECK_SINCE:-}"
DB="${ATK_DB:-atk.db}"
OUTPUT_TOL="${OUTPUT_TOL:-5.0}"
COST_TOL="${COST_TOL:-10.0}"
SKIP_RTK="${SKIP_RTK:-0}"

# ── 인수 파싱 ─────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --since)        SINCE="$2"; shift 2 ;;
        --db)           DB="$2"; shift 2 ;;
        --output-tolerance) OUTPUT_TOL="$2"; shift 2 ;;
        --cost-tolerance)   COST_TOL="$2"; shift 2 ;;
        --skip-rtk)     SKIP_RTK=1; shift ;;
        *) echo "알 수 없는 옵션: $1"; exit 1 ;;
    esac
done

echo "==================================================================="
echo " ATK ↔ ccusage / rtk gain 교차검증 하니스"
echo " DB: $DB | since: ${SINCE:-전체} | output_tol: ${OUTPUT_TOL}% | cost_tol: ${COST_TOL}%"
echo "==================================================================="
echo ""

# ── 사전 확인 ─────────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
    echo "❌ cargo가 PATH에 없습니다. Rust 개발 환경을 설정하세요." >&2
    exit 1
fi

if ! command -v npx &>/dev/null; then
    echo "❌ npx가 PATH에 없습니다. Node.js 환경을 설정하세요." >&2
    exit 1
fi

# ── 1. ATK 빌드 ───────────────────────────────────────────────
echo "🔨 ATK 빌드 중..."
cargo build --release -q 2>&1
ATK_BIN="./target/release/agent-token-tracker"
echo "   ✅ 빌드 완료: $ATK_BIN"
echo ""

# ── 2. ccusage 세션 데이터 수집 ───────────────────────────────
echo "📥 ccusage session 데이터 수집 중..."
CCUSAGE_ARGS="session --json"
if [[ -n "$SINCE" ]]; then
    CCUSAGE_ARGS="$CCUSAGE_ARGS --since $SINCE"
fi

CCUSAGE_JSON_FILE=$(mktemp /tmp/ccusage_cross_check_XXXXXX.json)
trap 'rm -f "$CCUSAGE_JSON_FILE"' EXIT

if ! npx ccusage $CCUSAGE_ARGS > "$CCUSAGE_JSON_FILE" 2>&1; then
    echo "❌ ccusage 실행 실패. 출력:" >&2
    cat "$CCUSAGE_JSON_FILE" >&2
    exit 1
fi

CCUSAGE_SESSION_COUNT=$(python3 -c "
import json, sys
data = json.load(open('$CCUSAGE_JSON_FILE'))
sessions = data.get('session', data) if isinstance(data, dict) else data
print(len([s for s in sessions if s.get('agent') == 'claude']))
" 2>/dev/null || echo "?")

echo "   ✅ ccusage 세션 수집 완료 (claude 세션: ${CCUSAGE_SESSION_COUNT}개)"
echo ""

# ── 3. ATK crosscheck 실행 ────────────────────────────────────
echo "🔍 ATK ↔ ccusage 교차검증 실행 중..."
CROSSCHECK_ARGS="--db $DB cross-check"
CROSSCHECK_ARGS="$CROSSCHECK_ARGS --ccusage-file $CCUSAGE_JSON_FILE"
CROSSCHECK_ARGS="$CROSSCHECK_ARGS --output-tolerance $OUTPUT_TOL"
CROSSCHECK_ARGS="$CROSSCHECK_ARGS --cost-tolerance $COST_TOL"
if [[ -n "$SINCE" ]]; then
    CROSSCHECK_ARGS="$CROSSCHECK_ARGS --since $SINCE"
fi

set +e
$ATK_BIN $CROSSCHECK_ARGS
CROSSCHECK_EXIT=$?
set -e

if [[ "$CROSSCHECK_EXIT" -eq 0 ]]; then
    echo ""
    echo "✅ 교차검증 통과: output 토큰 및 비용 모두 허용오차 내"
elif [[ "$CROSSCHECK_EXIT" -eq 2 ]]; then
    echo ""
    echo "⚠️  교차검증 경고: 허용오차 초과 불일치 감지됨 (위 리포트 참조)"
else
    echo ""
    echo "❌ ATK crosscheck 실행 중 오류 발생 (exit: $CROSSCHECK_EXIT)"
fi

# ── 4. RTK gain 확인 (선택적) ─────────────────────────────────
if [[ "$SKIP_RTK" -eq 0 ]]; then
    echo ""
    echo "📊 RTK gain 정보 수집 중..."
    echo "-------------------------------------------------------------------"
    echo "⚠️  RTK 측정 범위 caveat:"
    echo "  • rtk는 LLM 컨텍스트 압축 프록시입니다. 토큰 수를 직접 계산하지 않습니다."
    echo "  • rtk gain = (원본 출력 바이트 - 압축 출력 바이트) / 원본 출력 바이트"
    echo "  • 따라서 ATK 토큰 합계와 수치 기준 직접 대조는 불가합니다."
    echo "  • 아래는 ATK의 output 토큰과 rtk의 압축 효과를 참고용으로 병기합니다."
    echo "-------------------------------------------------------------------"

    # ATK 전체 output 토큰 합계 조회
    ATK_TOTAL_OUTPUT=$($ATK_BIN --db "$DB" report --dimension agent 2>/dev/null \
        | grep -E "^\|" | grep -v "에이전트" | grep -v "합계" \
        | awk -F'|' '{gsub(/[, ]/, "", $5); sum += $5} END {print sum+0}' \
        2>/dev/null || echo "조회 불가")

    echo ""
    echo "  ATK 전체 output 토큰 합계: ${ATK_TOTAL_OUTPUT}"
    echo "  RTK는 토큰 수 대신 압축률(gain)을 보고합니다."
    echo "  RTK 활용: export RTK_GAIN=1 후 AI 명령 실행 → gain% 표시됨"
    echo "-------------------------------------------------------------------"

    if command -v rtk &>/dev/null; then
        echo "  rtk 버전: $(rtk --version 2>/dev/null | head -1 || echo '확인 불가')"
    else
        echo "  ℹ️  rtk가 PATH에 없습니다. Homebrew로 설치: brew install rtk"
    fi
fi

echo ""
echo "==================================================================="
echo " 교차검증 완료"
echo "==================================================================="
exit "$CROSSCHECK_EXIT"
