import { formatResetDate, quotaPercentage } from "./providers";
import type { QuotaSnapshot } from "./types";

type QuotaCardProps = {
  snapshot: QuotaSnapshot;
};

export default function QuotaCard({ snapshot }: QuotaCardProps) {
  const percentage = quotaPercentage(snapshot);

  return (
    <section className="quota-card" aria-labelledby="quota-title">
      <div className="quota-card__heading">
        <div>
          <p className="quota-card__eyebrow">Disponible</p>
          <h2 id="quota-title">{snapshot.remaining} <span>/ {snapshot.limit}</span></h2>
        </div>
        <span className="quota-card__percentage">{Math.round(percentage)}%</span>
      </div>
      <div className="quota-card__progress" aria-label={`${Math.round(percentage)}% du quota restant`} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percentage)}>
        <span style={{ width: `${percentage}%` }} />
      </div>
      <div className="quota-card__footer">
        <span>{formatResetDate(snapshot.resetAt)}</span>
      </div>
    </section>
  );
}
