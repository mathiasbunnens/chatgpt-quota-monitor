import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { formatResetDate, quotaPercentage } from "../../features/quota/providers";
import type { QuotaPeriod, QuotaSnapshot } from "../../features/quota/types";
import "./dashboard.scss";

const USAGE_URL = "https://chatgpt.com/codex/cloud/settings/analytics#usage";

const periodLabels: Record<QuotaPeriod, string> = {
  "five-hour": "Limite 5 heures",
  weekly: "Limite globale",
  "reserve-weekly": "Réserve Luna",
};

function mergeSnapshots(
  current: Partial<Record<QuotaPeriod, QuotaSnapshot>>,
  incoming: QuotaSnapshot[],
) {
  return incoming.reduce<Partial<Record<QuotaPeriod, QuotaSnapshot>>>(
    (next, snapshot) => ({ ...next, [snapshot.period]: snapshot }),
    current,
  );
}

export default function Dashboard() {
  const [snapshots, setSnapshots] = useState<Partial<Record<QuotaPeriod, QuotaSnapshot>>>({});
  const [platform, setPlatform] = useState("desktop");
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [lastRefreshAt, setLastRefreshAt] = useState<Date | null>(null);

  const refreshQuota = async () => {
    setIsRefreshing(true);
    try {
      const storedSnapshots = await invoke<QuotaSnapshot[]>("get_quota_snapshots");
      setSnapshots((current) => mergeSnapshots(current, storedSnapshots));
      await invoke("request_quota_refresh");
      setLastRefreshAt(new Date());
    } finally {
      setIsRefreshing(false);
    }
  };

  useEffect(() => {
    void invoke<string>("get_platform").then(setPlatform).catch(() => undefined);

    let unlisten: (() => void) | undefined;
    let disposed = false;
    void listen<QuotaSnapshot>("quota-updated", (event) => {
      setSnapshots((current) => ({ ...current, [event.payload.period]: event.payload }));
    }).then((dispose) => {
      if (disposed) {
        dispose();
        return;
      }
      unlisten = dispose;
      void invoke<QuotaSnapshot[]>("get_quota_snapshots")
        .then((storedSnapshots) => {
          if (!disposed) setSnapshots((current) => mergeSnapshots(current, storedSnapshots));
        })
        .catch(() => undefined);
    });

    let unlistenDisconnected: (() => void) | undefined;
    void listen("quota-disconnected", () => {
      setSnapshots({});
    }).then((dispose) => {
      if (disposed) {
        dispose();
        return;
      }
      unlistenDisconnected = dispose;
    });

    return () => {
      disposed = true;
      unlisten?.();
      unlistenDisconnected?.();
    };
  }, []);

  const snapshot = snapshots["five-hour"];
  const visiblePeriods: QuotaPeriod[] = ["five-hour", "weekly", "reserve-weekly"];
  const availableSnapshots = visiblePeriods
    .map((period) => snapshots[period])
    .filter((current): current is QuotaSnapshot => Boolean(current));

  if (!snapshot) {
    return (
      <main className="dashboard dashboard--empty">
        <section className="dashboard__empty-state" aria-labelledby="empty-title">
          <span className="dashboard__empty-icon" aria-hidden="true">%</span>
          <h1 id="empty-title">En attente des quotas</h1>
          <p>Garde la page d’utilisation Codex ouverte pour activer la synchronisation.</p>
          <button type="button" onClick={() => void openUrl(USAGE_URL)}>
            Ouvrir la page d’utilisation
          </button>
          <button type="button" onClick={() => void refreshQuota()} disabled={isRefreshing}>
            {isRefreshing ? "Rechargement…" : "Recharger"}
          </button>
        </section>
      </main>
    );
  }

  return (
    <main className="dashboard">
      <header className="dashboard__header">
        <div>
          <p className="dashboard__eyebrow">Codex</p>
          <h1>Quotas</h1>
        </div>
        <div className="dashboard__actions">
          <span className="dashboard__status"><i /> Synchronisé</span>
          <button type="button" onClick={() => void refreshQuota()} disabled={isRefreshing}>
            {isRefreshing ? "Rechargement…" : "Recharger"}
          </button>
        </div>
      </header>
      <section className="dashboard__quota-list" aria-label="État des limites Codex">
        {availableSnapshots.map((current) => {
          const percentage = quotaPercentage(current);
          return (
            <article className="dashboard__quota" key={current.period}>
              <div className="dashboard__quota-heading">
                <span>{periodLabels[current.period]}</span>
                <strong>{Math.round(percentage)} % <small>restants</small></strong>
              </div>
              <div className="dashboard__progress" role="progressbar" aria-label={`${periodLabels[current.period]} : ${Math.round(percentage)} % restants`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percentage)}>
                <span style={{ width: `${percentage}%` }} />
              </div>
              <p>Réinitialisation {formatResetDate(current.resetAt)}</p>
            </article>
          );
        })}
      </section>
      <footer className="dashboard__footer">
        <span>{snapshot.model} · {platform}{lastRefreshAt ? ` · ${lastRefreshAt.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" })}` : ""}</span>
        <button type="button" onClick={() => void openUrl(USAGE_URL)}>Ouvrir Codex ↗</button>
      </footer>
    </main>
  );
}
