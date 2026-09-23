const SOURCE = "quota-codex-browser";

function parseResetAt(value) {
  const normalized = value
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[().,]/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase();
  const time = normalized.match(/(\d{1,2}):(\d{2})/);
  if (!time) return new Date().toISOString();

  const months = {
    jan: 0,
    janv: 0,
    feb: 1,
    fev: 1,
    mar: 2,
    mars: 2,
    apr: 3,
    avr: 3,
    may: 4,
    mai: 4,
    jun: 5,
    juin: 5,
    jul: 6,
    juil: 6,
    aug: 7,
    aout: 7,
    sep: 8,
    sept: 8,
    oct: 9,
    nov: 10,
    dec: 11,
  };

  const reset = new Date();
  reset.setHours(Number(time[1]), Number(time[2]), 0, 0);
  const datedReset = normalized.match(/(\d{1,2})\s+([a-z]+)(?:\s+(\d{4}))?/) ||
    normalized.match(/([a-z]+)\s+(\d{1,2})(?:\s+(\d{4}))?/);

  if (datedReset) {
    const dayFirst = /^\d/.test(datedReset[1]);
    const day = Number(dayFirst ? datedReset[1] : datedReset[2]);
    const month = months[dayFirst ? datedReset[2] : datedReset[1]];
    const year = Number(datedReset[3] || reset.getFullYear());
    if (month !== undefined) reset.setFullYear(year, month, day);
  } else if (reset.getTime() < Date.now()) {
    reset.setDate(reset.getDate() + 1);
  }

  return reset.toISOString();
}

function collectStatusLimits() {
  const text = (document.body?.innerText || "")
    .replace(/\u00a0/g, " ")
    .replace(/[’]/g, "'");
  const model = text.match(/(?:^|\n)\s*[│|]?\s*(?:Model|Modèle):\s+(.+)/i)?.[1]?.replace(/[│|]\s*$/, "").trim() || "Codex";
  const definitions = [
    ["(?:5h limit|5-hour usage limit|limite d'utilisation sur 5 heures?)", "five-hour"],
    ["(?:weekly limit|weekly usage limit|limite d'utilisation hebdomadaire)", "weekly"],
    ["(?:Luna Reserve Weekly limit|limite hebdomadaire Luna Reserve)", "reserve-weekly"],
  ];

  return definitions.flatMap(([labelPattern, period]) => {
    const match = new RegExp(
      `${labelPattern}[\\s:–-]*?(\\d{1,3})\\s*%\\s*(?:left|remaining|restants?)`,
      "i",
    ).exec(text);
    if (!match) return [];

    const followingText = text.slice(match.index + match[0].length, match.index + match[0].length + 240);
    const resetText = followingText.match(/(?:resets?(?:\s+at)?|réinitialisation)\s*:?\s*([^\n]+)/i)?.[1]?.trim();
    if (!resetText) return [];

    return [{
      model,
      remaining: Number(match[1]),
      limit: 100,
      reset_at: parseResetAt(resetText),
      checked_at: new Date().toISOString(),
      confidence: "high",
      period,
      reset_label: resetText,
    }];
  });
}

function publishStatus() {
  collectStatusLimits().forEach((payload) => {
    chrome.runtime.sendMessage({ type: SOURCE, payload });
  });
}

publishStatus();
setInterval(publishStatus, 10_000);
let publishTimeout;
new MutationObserver(() => {
  clearTimeout(publishTimeout);
  publishTimeout = setTimeout(publishStatus, 250);
}).observe(document.body, { childList: true, subtree: true, characterData: true });
