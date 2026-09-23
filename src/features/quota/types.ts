export type QuotaSource = "browser";
export type QuotaPeriod = "five-hour" | "weekly" | "reserve-weekly";

export type QuotaSnapshot = {
  model: string;
  remaining: number;
  limit: number;
  resetAt: string;
  checkedAt: string;
  source: QuotaSource;
  confidence: "low" | "medium" | "high";
  period: QuotaPeriod;
};
