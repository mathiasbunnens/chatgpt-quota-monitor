import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import type { CodexStatus } from "./types";

const pendingKey = "codex-pending-reset";
const outcomes: Record<string, string> = {
  reset: "Réinitialisation utilisée. Actualisation des quotas…",
  alreadyRedeemed: "Cette réinitialisation a déjà été appliquée. Actualisation des quotas…",
  nothingToReset: "Aucune limite éligible à réinitialiser pour le moment.",
  noCredit: "Aucune réinitialisation disponible sur ce compte.",
};

export default function ResetCredits({ status, onChange }: { status: CodexStatus | null; onChange: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const inFlight = useRef(false);
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState(() => localStorage.getItem(pendingKey));
  const [message, setMessage] = useState("");
  const [confirming, setConfirming] = useState(false);
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
      const outcome = await invoke<string>("redeem_codex_reset", { idempotencyKey: key });
      if (!outcomes[outcome]) throw new Error("Réponse inconnue.");
      localStorage.removeItem(pendingKey); setPending(null);
      setMessage(outcomes[outcome]); setConfirming(false); onChange();
    } catch {
      setMessage("Résultat non confirmé. Vérifie la connexion et la version de Codex, puis réessaie la même demande pour éviter de consommer deux réinitialisations.");
    } finally { inFlight.current = false; setBusy(false); }
  };
  const billing = async () => {
    try { await invoke("open_usage_details"); }
    catch { setMessage("Impossible d’ouvrir la page Codex."); }
  };
  return <section className="dashboard__quota reset-credits" aria-label="Réinitialisations supplémentaires">
    <div className="dashboard__quota-heading"><span>Réinitialisations disponibles</span><strong>{ready && count != null ? count : "—"}</strong></div>
    <p>{!ready ? "Connexion nécessaire." : count == null ? "Codex ne communique pas cette information. Une mise à jour de Codex peut être nécessaire." : count === 0 ? "Aucune réinitialisation en réserve." : "Utilise une réinitialisation pour restaurer une limite éligible de ton abonnement."}</p>
    <div className="dashboard__actions">
      <button disabled={!ready || busy || (!pending && !(count != null && count > 0))} onClick={() => setConfirming(true)}>{pending ? "Reprendre la demande" : "Utiliser une réinitialisation"}</button>
      {(count === 0 || count == null) && <button onClick={() => void billing()}>Crédits supplémentaires ↗</button>}
    </div>
    {(count === 0 || count == null) && <p>Les crédits payants prolongent l’utilisation selon ton offre. Achat et éventuelle recharge automatique se règlent sur la page Codex. L’application ne peut pas programmer de paiement avec l’interface disponible.</p>}
    {message && !confirming && <p role="status">{message}</p>}
    {confirming && <dialog className="settings-panel" ref={dialog} onCancel={(event) => { if (busy) event.preventDefault(); else setConfirming(false); }} aria-labelledby="reset-title">
      <h2 id="reset-title">{pending ? "Reprendre la réinitialisation ?" : "Utiliser une réinitialisation ?"}</h2>
      <p>Cette action consomme une réinitialisation disponible sur le compte Codex actuellement connecté, si une limite est éligible. Aucun achat n’est effectué.</p>
      {pending && <p>La demande précédente sera réutilisée pour éviter une double consommation. Conserve le même compte Codex pour cette tentative.</p>}
      {message && <p role="status">{message}</p>}
      <footer><button disabled={busy} onClick={() => setConfirming(false)}>Annuler</button><button className="primary" disabled={busy || !ready} onClick={() => void redeem()}>{busy ? "En cours…" : "Confirmer"}</button></footer>
    </dialog>}
  </section>;
}
