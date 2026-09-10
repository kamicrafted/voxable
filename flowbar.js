import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { playStart, playStop } from "./sounds.js";

const pill = document.getElementById("pill");
const mic = document.getElementById("mic");
const statusEl = document.getElementById("status");
const previewEl = document.getElementById("preview");
const copyBtn = document.getElementById("copy");

let isRecording = false;
let busy = false;
let lastResult = "";
let timerId = null;
let elapsed = 0;

// --- Idle collapse ---------------------------------------------------------
//
// Idle, the Flow Bar shrinks to a mic dot rather than disappearing. A dot is still
// a drag handle and a click target, so nothing has to reappear before it can be
// moved, and there is no invisible window sitting over the screen.

// Both states share a radius of 30: half of 60, which is a capsule at 296x60 and a
// circle at 60x60. That means the vibrancy mask never has to be rebuilt on resize —
// re-applying it was leaving the shape half-masked from the previous size, which
// showed up as a pill with square ends or a dot with a flat edge.
const EXPANDED = { w: 296, h: 60, radius: 30 };
const COLLAPSED = { w: 60, h: 60, radius: 30 };
const COLLAPSE_AFTER_MS = 1500;

let collapsed = false;
let collapseTimer = null;

// Settings are cached rather than fetched per dictation: starting a recording should
// not wait on an await. Rust emits settings-changed when they are saved.
let settings = {};

/// Report to the Rust log; release builds have no console to read.
function uiLog(level, message) {
  invoke("log_from_ui", { level, message }).catch(() => {});
}

async function refreshSettings() {
  try {
    settings = (await invoke("get_settings")) || {};
  } catch (e) {
    console.error("could not read settings:", e);
  }
}

/// Wait for the window server to actually apply a resize.
///
/// setSize resolves when the request is sent, not when the window has its new
/// frame, and the vibrancy mask is built from whatever size the window has when
/// setEffects runs. Applying the radius too early masks the expanded pill at the
/// dot's radius — which is what turned the pill back into a rounded rectangle.
function nextFrames(n = 3) {
  return new Promise((resolve) => {
    const step = (left) =>
      left <= 0 ? resolve() : requestAnimationFrame(() => step(left - 1));
    step(n);
  });
}

let appliedRadius = EXPANDED.radius; // matches tauri.conf.json at startup

async function setPillSize({ w, h, radius }) {
  const win = getCurrentWindow();
  await win.setSize(new LogicalSize(w, h));
  // Only touch the effect if the shape actually changes. With both states on the
  // same radius this never runs, which is the point.
  if (radius !== appliedRadius) {
    await nextFrames();
    try {
      await win.setEffects({ effects: ["popover"], state: "active", radius });
      appliedRadius = radius;
    } catch (e) {
      uiLog("error", `could not update the window effect: ${e}`);
    }
  }
  // Growing adds width to the right, so a dot near a display edge would expand
  // off-screen. Rust clamps it back.
  await invoke("fit_flowbar", { width: w, height: h }).catch(() => {});
}

async function expand() {
  cancelCollapse();
  if (!collapsed) return;
  try {
    collapsed = false;
    document.body.classList.remove("collapsed");
    await setPillSize(EXPANDED);
  } catch (e) {
    uiLog("error", `expand failed: ${e}`);
  }
}

async function collapse() {
  if (collapsed || isRecording || busy) return;
  try {
    collapsed = true;
    document.body.classList.add("collapsed");
    // Drop any leftover state class: a "done" dot rendered green instead of glass.
    setState("", "Ready");
    previewEl.textContent = "";
    copyBtn.hidden = true;
    await setPillSize(COLLAPSED);
  } catch (e) {
    collapsed = false;
    document.body.classList.remove("collapsed");
    uiLog("error", `collapse failed: ${e}`);
  }
}

function cancelCollapse() {
  if (collapseTimer) {
    clearTimeout(collapseTimer);
    collapseTimer = null;
  }
}

/// Collapse once nothing has happened for a beat. Called at every point the pill
/// becomes idle; recording and transcribing hold it open.
function scheduleCollapse() {
  cancelCollapse();
  collapseTimer = setTimeout(() => {
    collapseTimer = null;
    collapse();
  }, COLLAPSE_AFTER_MS);
}

function setState(cls, status) {
  pill.className = cls || "";
  if (status !== undefined) statusEl.textContent = status;
}

function startTimer() {
  elapsed = 0;
  statusEl.textContent = "0.0s";
  timerId = setInterval(() => {
    elapsed += 0.1;
    statusEl.textContent = `${elapsed.toFixed(1)}s`;
  }, 100);
}

function stopTimer() {
  if (timerId) clearInterval(timerId);
  timerId = null;
}

function showPreview(text) {
  lastResult = text;
  previewEl.textContent = text;
  copyBtn.hidden = false;
}

// --- Dictation pipeline ---

async function startRecording() {
  if (isRecording || busy) return;
  cancelCollapse();
  await expand();
  try {
    await invoke("start_recording");
    isRecording = true;
    if (settings.sound_enabled !== false) playStart();
    previewEl.textContent = "";
    copyBtn.hidden = true;
    setState("recording");
    startTimer();
  } catch (err) {
    setState("", `Error: ${err}`);
    console.error(err);
    scheduleCollapse();
  }
}

async function stopRecording() {
  if (!isRecording || busy) return;
  isRecording = false;
  busy = true;
  if (settings.sound_enabled !== false) playStop();
  stopTimer();

  try {
    const { samples } = await invoke("stop_recording");
    if (!samples) {
      setState("", "No audio");
      return;
    }
    setState("processing", "Transcribing…");
    const raw = await invoke("transcribe");
    if (!raw || raw.trim().length === 0) {
      setState("", "No speech detected");
      return;
    }
    setState("processing", "Cleaning up…");
    const cleaned = await invoke("cleanup");

    // Always put the result on the clipboard — that's the core feature.
    let copied = false;
    try {
      await invoke("copy_to_clipboard", { text: cleaned });
      copied = true;
    } catch (e) {
      console.error("clipboard failed:", e);
    }

    // Auto-paste into the field that still has focus (Flow Bar never took it).
    let pasted = false;
    if (copied) {
      try {
        const s = await invoke("get_settings");
        if (s.auto_paste) {
          await new Promise((r) => setTimeout(r, 60));
          await invoke("paste_to_active");
          pasted = true;
        }
      } catch (e) {
        console.error("auto-paste failed:", e);
      }
    }

    setState("done", pasted ? "Pasted ✓" : copied ? "Copied ✓" : "Done");
    showPreview(cleaned);
  } catch (err) {
    setState("", `Error: ${err}`);
    console.error(err);
  } finally {
    busy = false;
    scheduleCollapse();
  }
}

function toggle() {
  if (isRecording) stopRecording();
  else startRecording();
}

// Expanded, the mic is an ordinary button and the pill around it is the drag region.
// Collapsed, the dot is both — and they conflict: data-tauri-drag-region starts a
// native drag on mousedown and swallows the click, so the dot cannot be a drag region
// and a button at once. Decide from the gesture instead: move the pointer and it
// drags, press without moving and it starts dictation.
const DRAG_THRESHOLD_PX = 3;

mic.addEventListener("click", () => {
  if (collapsed) return; // handled by the gesture logic below
  toggle();
});

mic.addEventListener("mousedown", (e) => {
  if (!collapsed || e.button !== 0) return;
  const from = { x: e.screenX, y: e.screenY };
  let dragged = false;

  const onMove = (ev) => {
    if (dragged) return;
    const moved =
      Math.abs(ev.screenX - from.x) > DRAG_THRESHOLD_PX ||
      Math.abs(ev.screenY - from.y) > DRAG_THRESHOLD_PX;
    if (!moved) return;
    dragged = true;
    done();
    // Hands the gesture to the window server; no mouseup reaches us afterwards.
    getCurrentWindow()
      .startDragging()
      .catch((err) => uiLog("error", `startDragging failed: ${err}`));
  };

  const onUp = () => {
    done();
    if (!dragged) toggle();
  };

  function done() {
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
  }

  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
});

copyBtn.addEventListener("click", async () => {
  if (!lastResult) return;
  try {
    await invoke("copy_to_clipboard", { text: lastResult });
    setState("done", "Copied ✓");
  } catch (e) {
    console.error(e);
  }
});

// Right-click anywhere on the pill → native context menu (Rust builds it).
window.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  invoke("show_flowbar_menu").catch(console.error);
});

// The Rust global shortcut is the sole hotkey owner; it emits this to us.
listen("toggle-recording", toggle);

// Push-to-talk: the hotkey was held rather than tapped, so releasing it ends
// dictation. stopRecording() guards on its own state, so this is safely ignored if
// we are not recording or are already transcribing.
listen("stop-recording", () => {
  if (isRecording) stopRecording();
});

// Any dictation completing (e.g. initiated from the Hub) updates the preview.
listen("dictation-complete", (e) => {
  if (busy) return; // our own run already handled the UI
  expand();
  setState("done", "Done ✓");
  showPreview(e.payload);
  scheduleCollapse();
});

listen("settings-changed", refreshSettings);

// Start collapsed-after-idle like any other idle moment, and read settings once so
// the first dictation does not have to wait for them.
refreshSettings();
scheduleCollapse();

// --- Position persistence: debounce window moves, save the logical position. ---
//
// onMoved reports physical pixels; the position is stored and restored as logical
// points so it survives moving between displays with different scale factors.
// Saving physical is what pushed the pill off-screen on a 2x display.
const appWindow = getCurrentWindow();
let moveTimer = null;
appWindow.onMoved(({ payload }) => {
  if (moveTimer) clearTimeout(moveTimer);
  moveTimer = setTimeout(async () => {
    try {
      const factor = await appWindow.scaleFactor();
      invoke("set_flowbar_position", {
        position: { x: payload.x / factor, y: payload.y / factor },
      }).catch(console.error);
    } catch (err) {
      console.error(err);
    }
  }, 400);
});
