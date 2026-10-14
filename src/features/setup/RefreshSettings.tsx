import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import type { CodexStatus } from "../quota/types";

/** Refresh-frequency form (formerly a modal dialog), embedded in the Settings window. */
export default function RefreshSettings({ status, onChange }: {
  status: CodexStatus | null; onChange: () => void;
}) {
  const saved = String(status?.refreshSettings.customSeconds ?? "auto");
  const [value, setValue] = useState(saved);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const save = async () => {
    setSaving(true); setError(null); setMessage(null);
    try {
      await invoke("set_refresh_settings", { settings: { customSeconds: value === "auto" ? null : Number(value) } });
      setMessage("Enregistré."); onChange();
    } catch (cause) { setError(String(cause)); }
    finally { setSaving(false); }
  };
  return <div className="settings-form">
    <p className="settings-row"><span>Abonnement Codex détecté</span><strong>{status?.planType || "En attente de connexion"}</strong></p>
    <label htmlFor="refresh-frequency">Fréquence de synchronisation de Codex</label>
    <select id="refresh-frequency" value={value} onChange={(event) => { setValue(event.target.value); setMessage(null); }}>
      <option value="auto">Dynamique · abonnement et activité</option>
      <option value="30">Toutes les 30 secondes</option><option value="60">Toutes les minutes</option>
      <option value="120">Toutes les 2 minutes</option><option value="300">Toutes les 5 minutes</option>
      <option value="600">Toutes les 10 minutes</option>
    </select>
    <p>Codex détecté : {status?.activeInstances ?? "indisponible"} instance(s). Intervalle actuel : {status?.refreshSeconds ?? 120} s.</p>
    <p>Aucune instance : 5 min. Pour Plus, Pro et les offres d’équipe : 1 instance → 60 s ; 2–3 → 30 s ; 4 ou plus → 15 s. Pour Free ou un plan inconnu : 120 / 60 / 30 s.</p>
    <p>Estimation locale des applications et sessions CLI ouvertes, pas des conversations ni des requêtes en cours. Les processus auxiliaires et la connexion de ce moniteur sont exclus. Aucun contenu de conversation n’est lu.</p>
    <p>Le mode dynamique vérifie l’activité toutes les 10 secondes. Un intervalle personnalisé désactive cette adaptation. Votre choix est conservé au redémarrage.</p>
    <p>Claude n’a pas d’intervalle : ses limites sont transmises par Claude Code pendant qu’il tourne.</p>
    {status?.planType === "plus" && <p>Plus : la limite de 5 heures s’affiche en priorité. À 0 %, la réserve prend sa place lorsqu’elle est fournie par Codex.</p>}
    {error && <p role="alert">{error}</p>}
    {message && <p role="status">{message}</p>}
    <div className="settings-actions"><button className="primary" onClick={() => void save()} disabled={saving || value === saved}>{saving ? "Enregistrement…" : "Enregistrer"}</button></div>
  </div>;
}
