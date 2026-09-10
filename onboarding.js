// First-launch permission screen.
//
// Permissions can be granted outside this window (System Settings), and macOS sends
// no notification when they change, so the screen polls while it is open.

import { invoke } from "@tauri-apps/api/core";

const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => Array.from(document.querySelectorAll(sel));

const LABELS = {
  granted: "Allowed",
  denied: "Denied",
  "not-determined": "Not set",
  restricted: "Restricted",
  unknown: "Unknown",
};

function paintStep(name, state) {
  const badge = $(`.ob-status[data-status="${name}"]`);
  const step = $(`.ob-step[data-step="${name}"]`);
  if (!badge || !step) return;

  badge.textContent = LABELS[state] || LABELS.unknown;
  badge.dataset.state = state;
  step.dataset.state = state;

  // Once a permission is granted its buttons have nothing left to do.
  step.querySelectorAll("button").forEach((btn) => {
    btn.disabled = state === "granted";
  });
}

async function refresh() {
  let status;
  try {
    status = await invoke("permission_status");
  } catch (e) {
    console.error("permission_status failed:", e);
    return;
  }

  paintStep("microphone", status.microphone);
  paintStep("accessibility", status.accessibility ? "granted" : "not-determined");

  // The Globe-key tip only matters once fn can actually be watched for.
  $("#globe-tip").hidden = !status.accessibility;

  const ready = status.microphone === "granted";
  $('[data-action="done"]').textContent = ready
    ? "Start using Voxable"
    : "Continue without microphone";
}

const actions = {
  "request-microphone": () => invoke("request_microphone"),
  "request-accessibility": () => invoke("request_accessibility"),
  "open-microphone": () => invoke("open_privacy_settings", { pane: "microphone" }),
  "open-accessibility": () => invoke("open_privacy_settings", { pane: "accessibility" }),
  "open-keyboard": () => invoke("open_privacy_settings", { pane: "keyboard" }),
  skip: () => invoke("complete_onboarding"),
  done: () => invoke("complete_onboarding"),
};

$$("[data-action]").forEach((btn) => {
  btn.addEventListener("click", async () => {
    try {
      await actions[btn.dataset.action]();
    } catch (e) {
      console.error(`${btn.dataset.action} failed:`, e);
    }
    refresh();
  });
});

refresh();
setInterval(refresh, 1500);
window.addEventListener("focus", refresh);
