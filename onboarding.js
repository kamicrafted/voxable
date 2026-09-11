// First-launch permission screen.
//
// macOS: permissions can be granted outside this window (System Settings), and
// macOS sends no notification when they change, so the screen polls while it is
// open and auto-dismisses once both are granted.
//
// Windows: there are no OS-level permission prompts — microphone access is
// granted by the first `cpal` stream open, and auto-paste needs no extra grant.
// The screen still shows so the user can set a hotkey and see what the app
// does, but it does NOT auto-dismiss: the user clicks "Start using Voxable"
// when ready. Polling is skipped since there is nothing to wait for.

import { invoke } from "@tauri-apps/api/core";

const IS_MACOS = navigator.platform.toLowerCase().includes("mac");

const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => Array.from(document.querySelectorAll(sel));

// Set once the screen has decided to close itself, so polling cannot queue a
// second finish.
let finishing = false;

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

  // macOS only: both granted means there is nothing left to do on this screen,
  // so finish on its own rather than making the user hunt for the button.
  // Windows always shows "granted" (no permission prompts), so auto-dismissing
  // would flash the screen for 1.6 s and close it.
  if (ready && status.accessibility && !finishing && IS_MACOS) {
    finishing = true;
    $("#allset").hidden = false;
    $$(".ob-footer button").forEach((btn) => (btn.disabled = true));
    setTimeout(() => invoke("complete_onboarding").catch(console.error), 1600);
  }
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
if (IS_MACOS) {
  setInterval(refresh, 1500);
}
window.addEventListener("focus", refresh);
