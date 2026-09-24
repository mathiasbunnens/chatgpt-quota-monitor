const BRIDGE_URL = "http://127.0.0.1:48721/quota";
const CONNECTION_URL = "http://127.0.0.1:48721/connection";
const REFRESH_URL = "http://127.0.0.1:48721/refresh";
const REFRESH_ALARM = "quota-codex-refresh";
const REFRESH_TOKEN_KEY = "desktopRefreshToken";
const BACKGROUND_USAGE_TAB_KEY = "backgroundUsageTabId";
const LAST_BACKGROUND_RELOAD_KEY = "lastBackgroundReloadAt";
const BACKGROUND_RELOAD_INTERVAL_MS = 2 * 60 * 1_000;
const USAGE_PAGE_URL = "https://chatgpt.com/codex/cloud/settings/analytics#usage";
const USAGE_PAGE_PATTERNS = [
  "https://chatgpt.com/codex/settings/usage*",
  "https://chatgpt.com/codex/cloud/settings/analytics*",
  "https://chat.openai.com/codex/settings/usage*",
  "https://chat.openai.com/codex/cloud/settings/analytics*",
];

let refreshCheckInFlight = false;
let connectionCheckInFlight = false;

function isUsagePage(url = "") {
  return USAGE_PAGE_PATTERNS.some((pattern) => url.startsWith(pattern.replace(/\*$/, "")));
}

async function getStoredBackgroundTab() {
  const stored = await chrome.storage.local.get(BACKGROUND_USAGE_TAB_KEY);
  const tabId = stored[BACKGROUND_USAGE_TAB_KEY];
  if (!Number.isInteger(tabId)) return undefined;

  try {
    const tab = await chrome.tabs.get(tabId);
    if (isUsagePage(tab.url)) return tab;
  } catch {
    // The tab was closed while Brave was not running or while the extension reloaded.
  }

  await chrome.storage.local.remove(BACKGROUND_USAGE_TAB_KEY);
  return undefined;
}

async function findReusableBackgroundTab() {
  const tabs = await chrome.tabs.query({ url: USAGE_PAGE_PATTERNS });
  return tabs.find((tab) => !tab.active && tab.id !== undefined);
}

async function rememberBackgroundTab(tab) {
  if (tab.id === undefined) return undefined;
  await chrome.storage.local.set({ [BACKGROUND_USAGE_TAB_KEY]: tab.id });
  return chrome.tabs.update(tab.id, { autoDiscardable: false, pinned: true });
}

async function getBackgroundUsageTab({ createIfMissing = false } = {}) {
  const storedTab = await getStoredBackgroundTab();
  if (storedTab) return storedTab;

  const reusableTab = await findReusableBackgroundTab();
  if (reusableTab) return rememberBackgroundTab(reusableTab);
  if (!createIfMissing) return undefined;

  const createdTab = await chrome.tabs.create({
    url: USAGE_PAGE_URL,
    active: false,
    pinned: true,
  });
  return rememberBackgroundTab(createdTab);
}

async function isTabInForeground(tab) {
  if (!tab.active || tab.windowId === undefined) return false;
  try {
    const window = await chrome.windows.get(tab.windowId);
    return window.focused;
  } catch {
    return false;
  }
}

async function readUsagePage(tabId) {
  try {
    const response = await chrome.tabs.sendMessage(tabId, {
      type: "quota-codex-read-page",
    });
    return response?.ok === true;
  } catch {
    return false;
  }
}

async function backgroundReloadIsDue() {
  const stored = await chrome.storage.local.get(LAST_BACKGROUND_RELOAD_KEY);
  const lastReloadAt = stored[LAST_BACKGROUND_RELOAD_KEY];
  return !Number.isFinite(lastReloadAt) || Date.now() - lastReloadAt >= BACKGROUND_RELOAD_INTERVAL_MS;
}

async function reloadUsageTab(tabId) {
  await chrome.storage.local.set({ [LAST_BACKGROUND_RELOAD_KEY]: Date.now() });
  await chrome.tabs.reload(tabId);
}

async function syncBackgroundUsageTab(options = {}) {
  const tab = await getBackgroundUsageTab(options);
  if (tab?.id === undefined) return;

  if (tab.status === "loading") {
    await chrome.storage.local.set({ [LAST_BACKGROUND_RELOAD_KEY]: Date.now() });
    return;
  }

  const isForeground = await isTabInForeground(tab);
  const shouldReload =
    options.forceReload === true ||
    tab.discarded === true ||
    (!isForeground && (await backgroundReloadIsDue()));

  if (shouldReload) {
    await reloadUsageTab(tab.id);
    return;
  }

  if (!(await readUsagePage(tab.id))) await reloadUsageTab(tab.id);
}

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
    if (previousToken !== undefined || refreshToken > 0) {
      await syncBackgroundUsageTab({ createIfMissing: true, forceReload: true });
    }
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
  void syncBackgroundUsageTab({ forceReload: true }).catch(() => undefined);
});

chrome.runtime.onStartup.addListener(() => {
  void ensureRefreshAlarm();
  void checkForForcedRefresh();
  void reportConnectionStatus();
  void syncBackgroundUsageTab({ forceReload: true }).catch(() => undefined);
});

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === REFRESH_ALARM) {
    void checkForForcedRefresh();
    void reportConnectionStatus();
    void syncBackgroundUsageTab().catch(() => undefined);
  }
});

chrome.tabs.onRemoved.addListener((tabId) => {
  void chrome.storage.local.get(BACKGROUND_USAGE_TAB_KEY).then((stored) => {
    if (stored[BACKGROUND_USAGE_TAB_KEY] === tabId) {
      return chrome.storage.local.remove(BACKGROUND_USAGE_TAB_KEY);
    }
    return undefined;
  });
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
