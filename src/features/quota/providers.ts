import type { QuotaSnapshot } from "./types";

export function quotaPercentage(snapshot: QuotaSnapshot) {
  if (snapshot.limit <= 0) return 0;
  return Math.max(0, Math.min(100, (snapshot.remaining / snapshot.limit) * 100));
}

export function formatResetDate(value: string) {
  const date = new Date(value);
  if (!value || Number.isNaN(date.getTime())) return "non communiquée";
  return new Intl.DateTimeFormat("fr-FR", {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export function quotaLabel(snapshot: QuotaSnapshot) {
  return snapshot.label || ({
    "five-hour": "Limite 5 heures",
    weekly: "Limite hebdomadaire",
    "reserve-weekly": "Réserve Luna",
  } as Record<string, string>)[snapshot.period] || snapshot.period;
}
