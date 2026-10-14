import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import type { CodexStatus } from "./types";
import ExternalIcon from "../ui/ExternalIcon";

const pendingKey = "codex-pending-reset";
const outcomes: Record<string, string> = {
  reset: "Réinitialisation utilisée.",
  alreadyRedeemed: "Cette réinitialisation a déjà été appliquée.",
  nothingToReset: "Aucune limite éligible à réinitialiser pour le moment.",
  noCredit: "Aucune réinitialisation disponible sur ce compte.",
};

export function ResetIcon() {
  return <svg className="reset-icon" viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
    <path d="M13 8a5 5 0 1 1-1.5-3.6M13 2.5v3.2H9.8" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
  </svg>;
}

type Props = { status: CodexStatus | null; onChange: () => void; compact?: boolean };

export default function ResetCredits({ status, onChange, compact = false }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const inFlight = useRef(false);
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState(() => localStorage.getItem(pendingKey));
  const [message, setMessage] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [done, setDone] = useState(false);
  useEffect(() => { if (confirming) dialog.current?.showModal(); }, [confirming]);
  const count = status?.availableResets;
  const ready = status?.phase === "ready";
  const redeem = async () => {
    if (inFlight.current) return;
    inFlight.current = true; setBusy(true); setMessage("");
    try {
      // Persist before sending so a lost reply or restart cannot spend a second reset.
      const key = pending || crypto.randomUUID();
      localStorage.setItem(pendingKey, key); setPending(key);
      const { outcome, quotasRefreshed } = await invoke<{ outcome: string; quotasRefreshed: boolean }>("redeem_codex_reset", { idempotencyKey: key });
      if (!outcomes[outcome]) throw new Error("Réponse inconnue.");
      localStorage.removeItem(pendingKey); setPending(null);
      setMessage(outcomes[outcome] + (quotasRefreshed
        ? " Quotas actualisés."
        : " Impossible d’actualiser les quotas. Utilise Actualiser lorsque la connexion est rétablie ; ne consomme pas une autre réinitialisation pour cette tentative."));
      // Compact chip: the outcome is shown in the dialog, which stays open.
      if (compact) setDone(true); else setConfirming(false);
      onChange();
    } catch {
      setMessage("Résultat non confirmé. Vérifie la connexion et la version de Codex, puis réessaie la même demande pour éviter de consommer deux réinitialisations.");
    } finally { inFlight.current = false; setBusy(false); }
  };
  const billing = async () => {
    try { await invoke("open_usage_details"); }
    catch { setMessage("Impossible d’ouvrir la page Codex."); }
  };
  const dialogView = confirming && <dialog className="settings-panel" ref={dialog} onCancel={(event) => { if (busy) event.preventDefault(); else { setConfirming(false); setDone(false); } }} aria-labelledby="reset-title">
    <h2 id="reset-title">{pending ? "Reprendre la réinitialisation ?" : "Utiliser une réinitialisation ?"}</h2>
    <p>Cette action consomme une réinitialisation disponible sur le compte Codex actuellement connecté, si une limite est éligible. Aucun achat n’est effectué.</p>
    {pending && <p>La demande précédente sera réutilisée pour éviter une double consommation. Conserve le même compte Codex pour cette tentative.</p>}
    {message && <p role="status">{message}</p>}
    <footer>
      <button disabled={busy} onClick={() => { setConfirming(false); setDone(false); }}>{done ? "Fermer" : "Annuler"}</button>
      {!done && <button className="primary" disabled={busy || !ready} onClick={() => void redeem()}>{busy ? "En cours…" : "Confirmer"}</button>}
    </footer>
  </dialog>;
  if (compact) {
    const usable = ready && (!!pending || (count != null && count > 0));
    const shown = !ready || count == null ? "—" : String(count);
    return <>
      <button type="button" className="reset-chip" disabled={!usable || busy}
        onClick={() => { setMessage(""); setDone(false); setConfirming(true); }}
        aria-label={`Réinitialisations disponibles : ${shown}`}
        title={usable ? "Utiliser une réinitialisation" : "Aucune réinitialisation disponible"}>
        <ResetIcon /><span>{shown}</span>
      </button>
      {dialogView}
    </>;
  }
  return <section className="dashboard__quota reset-credits" aria-label="Réinitialisations supplémentaires">
    <div className="dashboard__quota-heading"><span>Réinitialisations disponibles</span><strong>{ready && count != null ? count : "—"}</strong></div>
    <p>{!ready ? "Connexion nécessaire." : count == null ? "Codex ne communique pas cette information. Une mise à jour de Codex peut être nécessaire." : count === 0 ? "Aucune réinitialisation en réserve." : "Utilise une réinitialisation pour restaurer une limite éligible de ton abonnement."}</p>
    <div className="dashboard__actions">
      <button disabled={!ready || busy || (!pending && !(count != null && count > 0))} onClick={() => setConfirming(true)}>{pending ? "Reprendre la demande" : "Utiliser une réinitialisation"}</button>
      {(count === 0 || count == null) && <button onClick={() => void billing()}>Crédits supplémentaires <ExternalIcon /></button>}
    </div>
    {(count === 0 || count == null) && <p>Les crédits payants prolongent l’utilisation selon ton offre. Achat et éventuelle recharge automatique se règlent sur la page Codex. L’application ne peut pas programmer de paiement avec l’interface disponible.</p>}
    {message && !confirming && <p role="status">{message}</p>}
    {dialogView}
  </section>;
}
