import { useMutation, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { queryKeys } from "../../lib/queryKeys";

// 키 상태 + 느린 쿼터를 함께 무효화 (자격 증명 변경은 쿼터 게이지에 영향)
function invalidateKeysAndQuota(qc: QueryClient) {
  qc.invalidateQueries({ queryKey: queryKeys.apiKeysStatus() });
  qc.invalidateQueries({ queryKey: queryKeys.subscriptionQuota() });
}

const providerLabel = (provider: string) => (provider === "anthropic" ? "Anthropic" : "OpenAI");

/** 설정 저장 (save_settings) — 설정/쿼터/DB 파생 무효화 */
export function useSaveSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: Record<string, unknown>) => invoke("save_settings", args),
    onSuccess: () => {
      toast.success("설정이 성공적으로 저장되었습니다.");
      qc.invalidateQueries({ queryKey: queryKeys.settings() });
      qc.invalidateQueries({ queryKey: queryKeys.subscriptionQuota() });
      qc.invalidateQueries({ predicate: (q) => q.queryKey[0] === "db" });
    },
    onError: (e) => toast.error(`설정 저장 실패: ${String(e)}`),
  });
}

/** API 키 저장 (save_api_key) */
export function useSaveApiKey() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { provider: "anthropic" | "openai"; key: string }) =>
      invoke("save_api_key", { provider: vars.provider, apiKey: vars.key, api_key: vars.key }),
    onSuccess: (_d, vars) => {
      toast.success(`${providerLabel(vars.provider)} API Key가 암호화되어 안전하게 보관되었습니다.`);
      qc.invalidateQueries({ queryKey: queryKeys.localCredentials() });
      invalidateKeysAndQuota(qc);
    },
    onError: (e) => toast.error(`API Key 저장 실패: ${String(e)}`),
  });
}

/** API 키 제거 (delete_api_key) */
export function useDeleteApiKey() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { provider: "anthropic" | "openai" }) =>
      invoke("delete_api_key", { provider: vars.provider }),
    onSuccess: (_d, vars) => {
      toast.success(`${providerLabel(vars.provider)} API Key가 제거되었습니다.`);
      invalidateKeysAndQuota(qc);
    },
    onError: (e) => toast.error(`API Key 제거 실패: ${String(e)}`),
  });
}

/** 자동 감지 자격 증명 바로 연동 (auto_apply_credential) */
export function useAutoApplyCredential() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { provider: string; rawValue: string }) =>
      invoke("auto_apply_credential", {
        provider: vars.provider,
        rawValue: vars.rawValue,
        raw_value: vars.rawValue,
      }),
    onSuccess: (_d, vars) => {
      toast.success(`${providerLabel(vars.provider)} 인증 정보가 성공적으로 연동 및 저장되었습니다.`);
      qc.invalidateQueries({ queryKey: queryKeys.localCredentials() });
      qc.invalidateQueries({ predicate: (q) => q.queryKey[0] === "db" });
      invalidateKeysAndQuota(qc);
    },
    onError: (e) => toast.error(`자동 연동 실패: ${String(e)}`),
  });
}
