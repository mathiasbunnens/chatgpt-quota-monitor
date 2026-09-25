import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import "./update.scss";

type UpdatePhase = "checking" | "available" | "downloading" | "ready" | "up-to-date" | "error";

type UpdateStatus = {
  phase: UpdatePhase;
  message: string;
  progress: number | null;
  currentVersion: string;
  nextVersion: string | null;
};

export default function UpdatePopup() {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void invoke<UpdateStatus>("get_update_status").then(setStatus);
    const subscription = listen<UpdateStatus>("update-status", (event) => setStatus(event.payload));
    return () => { void subscription.then((unlisten) => unlisten()); };
  }, []);

  const run = async (command: string) => {
    setBusy(true);
    try {
      await invoke(command);
    } finally {
      setBusy(false);
    }
  };

  const phase = status?.phase ?? "checking";
  const title = phase === "ready"
    ? "Mise à jour prête"
    : phase === "up-to-date"
      ? "Quota Codex est à jour"
      : phase === "error"
        ? "Mise à jour impossible"
        : "Mise à jour de Quota Codex";

  return <main className="update-popup">
    <p className="update-popup__eyebrow">Quota Codex {status?.currentVersion && `v${status.currentVersion}`}</p>
    <h1>{title}</h1>
    <p className="update-popup__message" role="status">{status?.message ?? "Recherche d’une nouvelle version…"}</p>
    {(phase === "downloading" || phase === "ready") && <div className="update-popup__progress">
      <progress max={100} value={status?.progress ?? 0} aria-label="Progression du téléchargement" />
      <span>{status?.progress ?? 0} %</span>
    </div>}
    <div className="update-popup__actions">
      {phase === "available" && <button disabled={busy} onClick={() => void run("start_update_install")}>Télécharger et installer</button>}
      {phase === "ready" && <button className="update-popup__primary" disabled={busy} onClick={() => void run("restart_application")}>Quitter et relancer</button>}
      {phase === "error" && <button disabled={busy} onClick={() => void run("start_update_check")}>Réessayer</button>}
      {(phase === "up-to-date" || phase === "error") && <button disabled={busy} onClick={() => void run("dismiss_update_window")}>Fermer</button>}
    </div>
  </main>;
}
