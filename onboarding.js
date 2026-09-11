// First-launch screen.
//
// macOS: shows Microphone + Accessibility permission grants. They can be granted
// outside this window (System Settings), and macOS sends no notification when
// they change, so the screen polls while open and auto-dismisses once both are
// granted.
//
// Windows: a desktop app needs no OS permission grants (mic is allowed for
// desktop apps by default, auto-paste needs nothing), and there is no
// Accessibility concept or `fn` key. So the permission machinery below does not
// run at all — Windows gets a static "how to use it" variant of the markup
// (gated by data-os in the HTML/CSS) and dismisses on the button click.

import { invoke } from "@tauri-apps/api/core";

// The platform was stamped on <html> by an inline head script before paint.
const IS_MACOS = document.documentElement.dataset.os === "mac";

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
    if (IS_MACOS) refresh();
  });
});

// macOS only: poll for permission changes and paint status. On Windows there is
// nothing to grant or poll — the static how-to variant is shown and the screen
// just waits for the "Start using Voxable" click.
if (IS_MACOS) {
  refresh();
  setInterval(refresh, 1500);
  window.addEventListener("focus", refresh);
}
