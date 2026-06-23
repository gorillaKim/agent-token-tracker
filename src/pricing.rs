//! 에이전트 및 모델별 토큰 단가 계산 모듈
//!
//! 사용자의 한국어 문서화 규칙에 맞춰 작성되었습니다.

use crate::model::Pricing;

/// 캐시 토큰과 모델 단가 정보를 기반으로 USD 비용을 정밀하게 연산합니다.
/// 모델 단가 정보가 없는 경우(None) 기본 fallback 단가(Claude 3.5 Sonnet)를 적용합니다.
pub fn calculate_cost_usd(
    pricing: Option<&Pricing>,
    input_tokens: u64,
    cache_read_input_tokens: u64,
    output_tokens: u64,
) -> f64 {
    // 단가 (1M 토큰당 가격)
    let (input_cost, output_cost, cached_cost) = match pricing {
        Some(p) => (p.input_cost_per_million, p.output_cost_per_million, p.cached_input_cost_per_million),
        None => (3.0, 15.0, 0.3), // Fallback: claude-3-5-sonnet 단가
    };

    // 캐시 리드 토큰은 전체 입력 토큰 중 일부
    let normal_input_tokens = if input_tokens >= cache_read_input_tokens {
        input_tokens - cache_read_input_tokens
    } else {
        0
    };

    let cost_input = (normal_input_tokens as f64) * input_cost / 1_000_000.0;
    let cost_cached = (cache_read_input_tokens as f64) * cached_cost / 1_000_000.0;
    let cost_output = (output_tokens as f64) * output_cost / 1_000_000.0;

    cost_input + cost_cached + cost_output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_cost_usd_with_pricing() {
        let pricing = Pricing::new(
            "claude-3-5-sonnet".to_string(),
            "anthropic".to_string(),
            3.0,
            15.0,
            0.3,
            "2026-06-23T00:00:00Z".to_string(),
        );

        // 입력 100,000 (캐시 30,000), 출력 50,000
        // normal_input = 70,000 * 3.0 / 1,000,000 = 0.21
        // cached_input = 30,000 * 0.3 / 1,000,000 = 0.009
        // output = 50,000 * 15.0 / 1,000,000 = 0.75
        // total = 0.21 + 0.009 + 0.75 = 0.969
        let cost = calculate_cost_usd(Some(&pricing), 100_000, 30_000, 50_000);
        assert!((cost - 0.969).abs() < 1e-9);
    }

    #[test]
    fn test_calculate_cost_usd_fallback() {
        // pricing이 None일 때 fallback 단가(Sonnet) 적용 확인
        let cost = calculate_cost_usd(None, 100_000, 30_000, 50_000);
        assert!((cost - 0.969).abs() < 1e-9);
    }
}
