export type QuotaSource = "codex";
export type QuotaPeriod = string;
export type QuotaSnapshot = {
  model: string;
  remaining: number;
  limit: number;
  resetAt: string;
  checkedAt: string;
  source: QuotaSource;
  confidence: "low" | "medium" | "high";
  period: QuotaPeriod;
  label?: string | null;
};
export type CodexStatus = {
  phase: "starting" | "missing" | "signed_out" | "ready" | "error" | "logging_in";
  message: string;
  executable: string | null;
  lastChecked: string | null;
  authUrl: string | null;
  userCode: string | null;
  planType: string | null;
  activeInstances: number | null;
  refreshSeconds: number;
  refreshSettings: { customSeconds: number | null };
};
