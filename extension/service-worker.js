const BRIDGE_URL = "http://127.0.0.1:48721/quota";

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
