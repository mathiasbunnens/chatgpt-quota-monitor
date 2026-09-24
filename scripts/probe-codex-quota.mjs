// Read-only capability probe. Never starts a thread, turn, or login.
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

const executable = process.argv[2] || "codex";
const child = spawn(executable, ["app-server"], {
  stdio: ["pipe", "pipe", "ignore"], windowsHide: true,
});
const lines = createInterface({ input: child.stdout });
let finished = false;
function finish(result, code = 0) {
  if (finished) return;
  finished = true;
  clearTimeout(timer);
  console.log(JSON.stringify(result, null, 2));
  lines.close();
  child.stdin.end();
  child.kill();
  process.exitCode = code;
}
const timer = setTimeout(() => finish({ ok: false, reason: "timeout" }, 1), 30000);
child.on("error", () => finish({ ok: false, reason: "could-not-start-codex" }, 1));
child.on("exit", () => { if (!finished) finish({ ok: false, reason: "codex-exited" }, 1); });
child.stdin.on("error", () => { if (!finished) finish({ ok: false, reason: "stdin-closed" }, 1); });
const send = (message) => child.stdin.write(JSON.stringify(message) + "\n");
lines.on("line", (line) => {
  let message;
  try { message = JSON.parse(line); } catch { return; }
  if (message.id === 1) {
    if (message.error) return finish({ ok: false, stage: "initialize", code: message.error.code }, 1);
    send({ method: "initialized", params: {} });
    send({ method: "account/rateLimits/read", id: 2 });
  }
  if (message.id === 2) {
    if (message.error) return finish({ ok: false, stage: "rateLimits", code: message.error.code }, 1);
    const result = message.result;
    const buckets = result.rateLimitsByLimitId || { codex: result.rateLimits };
    finish({ ok: true, buckets: Object.entries(buckets).map(([id, bucket]) => ({
      id,
      windows: [bucket?.primary, bucket?.secondary].filter(Boolean).map((window) => ({
        usedPercent: window.usedPercent, windowDurationMins: window.windowDurationMins,
        resetsAt: window.resetsAt,
      })),
    })) });
  }
});
send({ method: "initialize", id: 1, params: { clientInfo: { name: "quota_monitor_probe", version: "0.1.0" } } });
