const BRIDGE_URL = "http://127.0.0.1:48721/quota";
const CONNECTION_URL = "http://127.0.0.1:48721/connection";
const REFRESH_URL = "http://127.0.0.1:48721/refresh";
const REFRESH_ALARM = "quota-codex-refresh";
const REFRESH_TOKEN_KEY = "desktopRefreshToken";
const USAGE_PAGE_PATTERNS = [
  "https://chatgpt.com/codex/settings/usage*",
  "https://chatgpt.com/codex/cloud/settings/analytics*",
  "https://chat.openai.com/codex/settings/usage*",
  "https://chat.openai.com/codex/cloud/settings/analytics*",
];

let refreshCheckInFlight = false;
let connectionCheckInFlight = false;

async function reportConnectionStatus() {
  if (connectionCheckInFlight) return;
  connectionCheckInFlight = true;

  try {
    const tabs = await chrome.tabs.query({ url: USAGE_PAGE_PATTERNS });
    await fetch(CONNECTION_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ connected: tabs.length > 0 }),
    });
  } catch {
    // The desktop app may not be running yet.
  } finally {
    connectionCheckInFlight = false;
  }
}

async function reloadUsagePages() {
  const tabs = await chrome.tabs.query({ url: USAGE_PAGE_PATTERNS });
  await Promise.all(
    tabs.flatMap((tab) => (tab.id === undefined ? [] : [chrome.tabs.reload(tab.id)])),
  );
}

async function checkForForcedRefresh() {
  if (refreshCheckInFlight) return;
  refreshCheckInFlight = true;

  try {
    const response = await fetch(REFRESH_URL, { cache: "no-store" });
    if (!response.ok) return;

    const { refreshToken } = await response.json();
    if (!Number.isSafeInteger(refreshToken)) return;

    const stored = await chrome.storage.local.get(REFRESH_TOKEN_KEY);
    const previousToken = stored[REFRESH_TOKEN_KEY];
    if (previousToken === refreshToken) return;

    await chrome.storage.local.set({ [REFRESH_TOKEN_KEY]: refreshToken });
    if (previousToken !== undefined) await reloadUsagePages();
  } catch {
    // The desktop app may not be running yet.
  } finally {
    refreshCheckInFlight = false;
  }
}

async function ensureRefreshAlarm() {
  const alarm = await chrome.alarms.get(REFRESH_ALARM);
  if (!alarm) {
    await chrome.alarms.create(REFRESH_ALARM, { periodInMinutes: 0.5 });
  }
}

chrome.runtime.onInstalled.addListener(() => {
  void ensureRefreshAlarm();
  void checkForForcedRefresh();
  void reportConnectionStatus();
  void reloadUsagePages().catch(() => undefined);
});

chrome.runtime.onStartup.addListener(() => {
  void ensureRefreshAlarm();
  void checkForForcedRefresh();
  void reportConnectionStatus();
});

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === REFRESH_ALARM) {
    void checkForForcedRefresh();
    void reportConnectionStatus();
  }
});

chrome.tabs.onRemoved.addListener(() => {
  void reportConnectionStatus();
});

chrome.tabs.onUpdated.addListener((_tabId, changeInfo) => {
  if (changeInfo.status || changeInfo.url) void reportConnectionStatus();
});

void ensureRefreshAlarm();
void checkForForcedRefresh();
void reportConnectionStatus();
setInterval(() => {
  void checkForForcedRefresh();
  void reportConnectionStatus();
}, 2_000);

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message?.type !== "quota-codex-browser") return undefined;

  if (!message.payload?.limit) {
    sendResponse({ ok: false, reason: "quota-not-detected" });
    return undefined;
  }

  fetch(BRIDGE_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(message.payload),
  })
    .then((response) => sendResponse({ ok: response.ok }))
    .catch(() => sendResponse({ ok: false, reason: "desktop-unavailable" }));

  return true;
});
