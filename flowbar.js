import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

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
  try {
    await invoke("start_recording");
    isRecording = true;
    previewEl.textContent = "";
    copyBtn.hidden = true;
    setState("recording");
    startTimer();
  } catch (err) {
    setState("", `Error: ${err}`);
    console.error(err);
  }
}

async function stopRecording() {
  if (!isRecording || busy) return;
  isRecording = false;
  busy = true;
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
  }
}

function toggle() {
  if (isRecording) stopRecording();
  else startRecording();
}

mic.addEventListener("click", toggle);

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
  setState("done", "Done ✓");
  showPreview(e.payload);
});

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
