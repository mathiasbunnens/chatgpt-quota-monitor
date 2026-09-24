import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import CodexConnection from "../../features/setup/CodexConnection";
import { formatResetDate, quotaLabel, quotaPercentage } from "../../features/quota/providers";
import type { CodexStatus, QuotaSnapshot } from "../../features/quota/types";
import "./dashboard.scss";

export default function Dashboard() {
  const [snapshots, setSnapshots] = useState<QuotaSnapshot[]>([]);
  const [status, setStatus] = useState<CodexStatus | null>(null);
  const [showSetup, setShowSetup] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [revision, setRevision] = useState(0);
  const reload = useCallback(() => setRevision((value) => value + 1), []);

  useEffect(() => {
    let disposed = false;
    void invoke<string>("get_platform").then((platform) => {
      if (!disposed) document.documentElement.dataset.platform = platform;
    }).catch(() => undefined);
    return () => { disposed = true; };
  }, []);

  useEffect(() => {
    let disposed = false;
    let timer: number;
    const poll = async () => {
      try {
        const [nextSnapshots, nextStatus] = await Promise.all([
          invoke<QuotaSnapshot[]>("get_quota_snapshots"),
          invoke<CodexStatus>("get_codex_status"),
        ]);
        if (!disposed) { setSnapshots(nextSnapshots); setStatus(nextStatus); }
      } catch {
        if (!disposed) setError("Impossible de communiquer avec l’application. Relance Quota Codex.");
      } finally {
        if (!disposed) timer = window.setTimeout(() => void poll(), 2000);
      }
    };
    void poll();
    return () => { disposed = true; window.clearTimeout(timer); };
  }, [revision]);

  const refresh = async () => {
    setRefreshing(true); setError(null);
    try { await invoke("request_quota_refresh"); reload(); }
    catch (cause) { setError(String(cause)); }
    finally { setRefreshing(false); }
  };
  const openDetails = async () => {
    setError(null);
    try { await invoke("open_selected_usage"); }
    catch (cause) { setError(String(cause)); }
  };
  const usingFallback = snapshots.some((snapshot) => snapshot.source === "browser");
  const needsSetup = snapshots.length === 0 && status?.phase !== "ready";

  return <main className="dashboard">
    <header className="dashboard__header">
      <div><p className="dashboard__eyebrow">Codex</p><h1>Quotas</h1></div>
      <div className="dashboard__actions">
        <button onClick={() => void refresh()} disabled={refreshing}>{refreshing ? "Actualisation…" : "Actualiser"}</button>
        <button onClick={() => setShowSetup((value) => !value)} aria-expanded={showSetup}>{showSetup ? "Masquer les réglages" : "Connexion"}</button>
      </div>
    </header>
    <p className="dashboard__source" role="status">
      {usingFallback ? "Source : extension navigateur (secours)" : status?.phase === "ready" ? "Source : Codex · Synchronisé" : status?.message || "Connexion à Codex…"}
    </p>
    {error && <p role="alert">{error}</p>}
    {usingFallback && <p className="dashboard__source">{status?.message}</p>}
    <section className="dashboard__quota-list" aria-label="État des limites Codex">
      {snapshots.map((snapshot) => {
        const percentage = quotaPercentage(snapshot);
        const label = quotaLabel(snapshot);
        return <article className="dashboard__quota" key={snapshot.period}>
          <div className="dashboard__quota-heading"><span>{label}</span><strong>{Math.round(percentage)} % <small>restants</small></strong></div>
          <div className={"dashboard__progress dashboard__progress--" + (percentage <= 20 ? "low" : percentage <= 40 ? "warning" : "healthy")}
            role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percentage)}>
            <span style={{ width: percentage + "%" }} />
          </div>
          <p>Réinitialisation : {formatResetDate(snapshot.resetAt)}</p>
        </article>;
      })}
    </section>
    {status?.phase === "ready" && !snapshots.length && <p>{status.message}</p>}
    <footer className="dashboard__footer">
      <span>{snapshots[0]?.checkedAt ? "Vérifié : " + new Date(snapshots[0].checkedAt).toLocaleTimeString("fr-FR") : "Aucune donnée reçue"}</span>
      <button onClick={() => void openDetails()}>Voir les détails sur Codex ↗</button>
    </footer>
    {(showSetup || needsSetup) && <CodexConnection status={status} onChange={reload} />}
  </main>;
}
