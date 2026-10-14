import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import CodexConnection from "../setup/CodexConnection";
import RefreshSettings from "../setup/RefreshSettings";
import type { ClaudeSnapshot, CodexStatus, IndicatorChoice, Preferences } from "../quota/types";
import "../../pages/Dashboard/dashboard.scss";
import "./settings.scss";

const tabs = [
  { id: "providers", label: "Fournisseurs" },
  { id: "menubar", label: "Barre des menus" },
  { id: "refresh", label: "Actualisation" },
] as const;
type TabId = (typeof tabs)[number]["id"];

const indicatorOptions: { value: IndicatorChoice; label: string }[] = [
  { value: "codex", label: "Codex" },
  { value: "claude", label: "Claude" },
  { value: "both", label: "Les deux" },
];

const claudeStates: Record<ClaudeSnapshot["state"], string> = {
  disabled: "Désactivé.",
  not_configured: "Claude Code non installé : installez-le puis lancez claude auth login.",
  waiting_for_data: "Lecture des quotas via Claude Code…",
  ready: "Quotas lus à la demande via Claude Code.",
  stale: "Dernières données anciennes.",
  error: "Lecture impossible : vérifiez la connexion et la version de Claude Code.",
};

export default function Settings() {
  const [tab, setTab] = useState<TabId>("providers");
  const [prefs, setPrefs] = useState<Preferences | null>(null);
  const [status, setStatus] = useState<CodexStatus | null>(null);
  const [claude, setClaude] = useState<ClaudeSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [revision, setRevision] = useState(0);
  const tabRefs = useRef<Record<string, HTMLButtonElement | null>>({});
  const reload = useCallback(() => setRevision((value) => value + 1), []);

  useEffect(() => {
    const quotas = listen("quota-updated", reload).catch(() => () => {});
    return () => { void quotas.then((unlisten) => unlisten()); };
  }, [reload]);

  useEffect(() => {
    let disposed = false;
    let timer: number;
    const poll = async () => {
      const [p, s, k] = await Promise.allSettled([
        invoke<Preferences>("get_preferences"),
        invoke<CodexStatus>("get_codex_status"),
        invoke<ClaudeSnapshot>("get_claude_snapshot"),
      ]);
      if (disposed) return;
      if (p.status === "fulfilled") setPrefs(p.value);
      if (s.status === "fulfilled") setStatus(s.value);
      if (k.status === "fulfilled") setClaude(k.value);
      timer = window.setTimeout(() => void poll(), 2000);
    };
    void poll();
    return () => { disposed = true; window.clearTimeout(timer); };
  }, [revision]);

  const update = async (patch: Partial<Preferences>) => {
    if (!prefs) return;
    setError(null); setBusy(true);
    try { setPrefs(await invoke<Preferences>("set_preferences", { prefs: { ...prefs, ...patch } })); reload(); }
    catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  };
  const relay = async (command: "install_claude_relay" | "uninstall_claude_relay") => {
    setError(null); setBusy(true);
    try { setClaude(await invoke<ClaudeSnapshot>(command)); reload(); }
    catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  };

  const onTabKey = (event: KeyboardEvent, index: number) => {
    const move = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : event.key === "Home" ? -index : event.key === "End" ? tabs.length - 1 - index : 0;
    if (!move) return;
    event.preventDefault();
    const next = tabs[(index + move + tabs.length) % tabs.length];
    setTab(next.id); tabRefs.current[next.id]?.focus();
  };

  if (!prefs) return <main className="dashboard settings-page"><p role="status">Chargement…</p></main>;
  const claudeConfigured = !!claude?.configured;

  return <main className="dashboard settings-page">
    <div role="tablist" aria-label="Catégories de réglages" className="settings-tabs">
      {tabs.map((item, index) => <button key={item.id} type="button" role="tab" id={"tab-" + item.id}
        ref={(node) => { tabRefs.current[item.id] = node; }}
        aria-selected={tab === item.id} aria-controls={"panel-" + item.id}
        tabIndex={tab === item.id ? 0 : -1} onClick={() => setTab(item.id)} onKeyDown={(event) => onTabKey(event, index)}>{item.label}</button>)}
    </div>
    {error && <p role="alert">{error}</p>}

    {tab === "providers" && <div role="tabpanel" id="panel-providers" aria-labelledby="tab-providers" className="settings-panel-body">
      <section className="settings-group" aria-labelledby="codex-heading">
        <h2 id="codex-heading">Codex</h2>
        <label className="settings-check"><input type="checkbox" checked={prefs.codexEnabled} disabled={busy}
          onChange={(event) => void update({ codexEnabled: event.target.checked })} />Activer Codex</label>
        <p className="settings-state" role="status">{prefs.codexEnabled ? status?.message || "Connexion à Codex…" : "Désactivé : aucune connexion ni sondage."}</p>
        {prefs.codexEnabled && <CodexConnection status={status} onChange={reload} />}
      </section>
      <section className="settings-group" aria-labelledby="claude-heading">
        <h2 id="claude-heading">Claude</h2>
        <label className="settings-check"><input type="checkbox" checked={prefs.claudeEnabled} disabled={busy}
          onChange={(event) => void update({ claudeEnabled: event.target.checked })} />Activer Claude</label>
        <p className="settings-state" role="status">{prefs.claudeEnabled ? (claude ? claudeStates[claude.state] : "Lecture…") : "Désactivé : aucune lecture."}</p>
        {prefs.claudeEnabled && <>
          <p>Source : protocole JSON du CLI officiel Claude Code (get_usage). Lecture à la demande et automatique, même hors conversation. Le CLI gère sa connexion ; Quota Monitor ne récupère aucun jeton et ne modifie pas votre statusLine.</p>
          <p>Connexion : lancez <code>claude auth login</code> dans votre terminal. Claude Code 2.1.299 est testé ; le protocole get_usage est expérimental.</p>
          <div className="settings-actions">
            <button type="button" disabled={busy} onClick={() => void invoke("request_claude_refresh").then(reload)}>Actualiser Claude</button>
            <button type="button" disabled={busy} onClick={() => void relay("uninstall_claude_relay")}>Retirer l’ancien relais (si installé)</button>
          </div>
          {claude?.observedAt && <p>Dernière lecture réussie : {new Date(claude.observedAt).toLocaleString()}.</p>}
          {claude?.message && <p role="status">{claude.message}</p>}
        </>}
      </section>
    </div>}

    {tab === "menubar" && <div role="tabpanel" id="panel-menubar" aria-labelledby="tab-menubar" className="settings-panel-body">
      <fieldset className="settings-group" disabled={busy}>
        <legend>Indicateur de la barre des menus</legend>
        {indicatorOptions.map((option) => <label className="settings-check" key={option.value}>
          <input type="radio" name="indicator" value={option.value} checked={prefs.indicator === option.value}
            onChange={() => void update({ indicator: option.value })} />{option.label}
        </label>)}
        <p>L’indicateur affiche le quota restant de la limite courte (5 heures). Si le fournisseur choisi est désactivé ou non configuré, l’autre fournisseur actif est affiché à sa place ; sans fournisseur actif, l’indicateur affiche « -- ». Une erreur sur un compte activé reste visible (« ! »), même s’il n’est pas choisi.</p>
        <p>Fournisseurs actifs : {[prefs.codexEnabled && "Codex", prefs.claudeEnabled && claudeConfigured && "Claude"].filter(Boolean).join(", ") || "aucun"}.</p>
      </fieldset>
    </div>}

    {tab === "refresh" && <div role="tabpanel" id="panel-refresh" aria-labelledby="tab-refresh" className="settings-panel-body">
      <section className="settings-group"><h2>Actualisation</h2>
        <RefreshSettings key={String(status?.refreshSettings.customSeconds ?? "auto")} status={status} onChange={reload} />
      </section>
    </div>}
  </main>;
}
