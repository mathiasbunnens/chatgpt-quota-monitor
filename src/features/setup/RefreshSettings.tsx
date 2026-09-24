import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import type { CodexStatus } from "../quota/types";

export default function RefreshSettings({ status, onClose, onChange }: {
  status: CodexStatus | null; onClose: () => void; onChange: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [value, setValue] = useState(String(status?.refreshSettings.customSeconds ?? "auto"));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { dialog.current?.showModal(); }, []);
  const save = async () => {
    setSaving(true); setError(null);
    try {
      await invoke("set_refresh_settings", { settings: { customSeconds: value === "auto" ? null : Number(value) } });
      onChange(); onClose();
    } catch (cause) { setError(String(cause)); }
    finally { setSaving(false); }
  };
  return <dialog className="settings-panel" ref={dialog} onCancel={onClose} aria-labelledby="refresh-title">
    <header><div><p className="dashboard__eyebrow">Préférences</p><h2 id="refresh-title">Actualisation</h2></div>
      <button onClick={onClose} aria-label="Fermer les réglages">✕</button></header>
    <section className="settings-panel__account"><span>Abonnement détecté</span><strong>{status?.planType || "En attente de connexion"}</strong></section>
    <label htmlFor="refresh-frequency">Fréquence de synchronisation</label>
    <select id="refresh-frequency" value={value} onChange={(event) => setValue(event.target.value)}>
      <option value="auto">Dynamique · abonnement et activité</option>
      <option value="30">Toutes les 30 secondes</option><option value="60">Toutes les minutes</option>
      <option value="120">Toutes les 2 minutes</option><option value="300">Toutes les 5 minutes</option>
      <option value="600">Toutes les 10 minutes</option>
    </select>
    <p>Codex détecté : {status?.activeInstances ?? "indisponible"} instance(s). Intervalle actuel : {status?.refreshSeconds ?? 120} s.</p>
    <p>Aucune instance : 5 min. Pour Plus, Pro et les offres d’équipe : 1 instance → 60 s ; 2–3 → 30 s ; 4 ou plus → 15 s. Pour Free ou un plan inconnu : 120 / 60 / 30 s.</p>
    <p>Estimation locale des applications et sessions CLI ouvertes, pas des conversations ni des requêtes en cours. Les processus auxiliaires et la connexion de ce moniteur sont exclus. Aucun contenu de conversation n’est lu.</p>
    <p>Le mode dynamique vérifie l’activité toutes les 10 secondes. Un intervalle personnalisé désactive cette adaptation. Votre choix est conservé au redémarrage. Le bouton Actualiser reste disponible à tout moment.</p>
    {status?.planType === "plus" && <p className="settings-panel__account">Plus : la limite de 5 heures s’affiche en priorité. À 0 %, la réserve prend sa place lorsqu’elle est fournie par Codex.</p>}
    {error && <p role="alert">{error}</p>}
    <footer><button onClick={onClose}>Annuler</button><button className="primary" onClick={() => void save()} disabled={saving}>{saving ? "Enregistrement…" : "Enregistrer"}</button></footer>
  </dialog>;
}
