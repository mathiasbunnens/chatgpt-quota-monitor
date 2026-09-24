import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import "./setup.scss";

type Browser = { id: string; name: string; extensionsUrl: string };
type SetupInfo = { extensionPath: string; browsers: Browser[]; selectedBrowser: string | null };
type Connection = { extensionConnected: boolean; usagePageOpen: boolean; bridgeError: string | null };

export default function BrowserSetup() {
  const [setup, setSetup] = useState<SetupInfo | null>(null);
  const [browser, setBrowser] = useState("");
  const [connection, setConnection] = useState<Connection | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const loadSetup = async () => {
    setError(null);
    try {
      const result = await invoke<SetupInfo>("get_browser_setup");
      setSetup(result);
      setBrowser(result.browsers.some((item) => item.id === result.selectedBrowser)
        ? result.selectedBrowser! : result.browsers[0]?.id ?? "");
    } catch (cause) { setError(String(cause)); }
  };

  useEffect(() => { void loadSetup(); }, []);
  useEffect(() => {
    let disposed = false;
    const update = async () => {
      try {
        const status = await invoke<Connection>("get_connection_status");
        if (!disposed) setConnection(status);
      } catch (cause) { if (!disposed) setError(String(cause)); }
    };
    void update();
    const timer = window.setInterval(() => void update(), 2000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, []);

  const run = async (command: string, args?: Record<string, string>) => {
    setBusy(true);
    setError(null);
    try { await invoke(command, args); }
    catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  };

  return <section className="browser-setup" aria-labelledby="setup-title">
    <h1 id="setup-title">Connecter le navigateur</h1>
    <p>L’extension est incluse dans l’application. Termine son installation dans le navigateur où tu utilises ChatGPT.</p>
    {error && <p role="alert">{error}</p>}
    {!setup && <button onClick={() => void loadSetup()}>Préparer l’extension</button>}
    {setup && <>
      <label htmlFor="setup-browser">Navigateur</label>
      <select id="setup-browser" value={browser} onChange={(event) => setBrowser(event.target.value)} disabled={busy || !setup.browsers.length}>
        {!setup.browsers.length && <option value="">Aucun navigateur détecté</option>}
        {setup.browsers.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
      </select>
      <ol>
        <li>
          <p>Ouvre la page des extensions.</p>
          {browser ? <>
            <button disabled={busy} onClick={() => void run("open_browser_setup", { browserId: browser, page: "extensions" })}>Ouvrir les extensions</button>
            <small>Si la page ne s’ouvre pas, colle cette adresse dans ce navigateur : <code>{setup.browsers.find((item) => item.id === browser)?.extensionsUrl}</code></small>
          </> : <p>Ouvre manuellement <code>chrome://extensions</code>, <code>edge://extensions</code> ou <code>brave://extensions</code> dans un navigateur Chromium. Pour une installation Flatpak ou Snap, autorise l’accès au dossier ci-dessous.</p>}
        </li>
        <li>
          <p>Active <strong>Mode développeur</strong>, puis clique sur <strong>Charger l’extension non empaquetée</strong> (Load unpacked).</p>
          <p>Sélectionne ce dossier :</p>
          <input aria-label="Dossier de l’extension" readOnly value={setup.extensionPath} onFocus={(event) => event.currentTarget.select()} />
          <button disabled={busy} onClick={() => void run("reveal_extension_folder")}>Ouvrir le dossier</button>
          <small>Le navigateur exige ces clics. Ce dossier reste au même emplacement après une mise à jour ; recharge alors l’extension depuis sa page.</small>
        </li>
        <li>
          <p>Connecte-toi à ChatGPT dans le même profil de navigateur, puis ouvre la page d’utilisation Codex.</p>
          <button disabled={busy || !browser} onClick={() => void run("open_browser_setup", { browserId: browser, page: "usage" })}>Ouvrir Codex dans ce navigateur</button>
          {!browser && <small>Adresse : https://chatgpt.com/codex/cloud/settings/analytics#usage</small>}
        </li>
      </ol>
    </>}
    <div className="browser-setup__status" role="status" aria-live="polite">
      {connection?.bridgeError ? connection.bridgeError : connection?.extensionConnected
        ? connection.usagePageOpen
          ? "Extension connectée. Page Codex détectée ; en attente des quotas. Vérifie ta connexion et recharge la page si nécessaire."
          : "Extension connectée. Ouvre maintenant la page d’utilisation Codex."
        : "En attente de l’extension… Les quotas apparaîtront automatiquement après connexion."}
    </div>
  </section>;
}
