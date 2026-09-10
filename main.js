import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  DEFAULT_HOTKEY,
  IS_MAC,
  hotkeyFromEvent,
  hotkeyParts,
  renderHotkey,
} from "./hotkeys.js";
import { audioContext, playBeep } from "./sounds.js";

const $ = (sel) => document.querySelector(sel);

/// Report to the Rust log; release builds have no console to read.
function uiLog(level, message) {
  invoke("log_from_ui", { level, message }).catch(() => {});
}

// Nothing in a release webview surfaces an exception — no console, no devtools — so
// a broken tab looks like a tab that does nothing. Send them to the app log.
window.addEventListener("error", (e) => {
  uiLog("error", `uncaught: ${e.message} at ${e.filename}:${e.lineno}:${e.colno}`);
});
window.addEventListener("unhandledrejection", (e) => {
  uiLog("error", `unhandled rejection: ${e.reason}`);
});

const $$ = (sel) => Array.from(document.querySelectorAll(sel));

// Keep the last-loaded settings so saves send the FULL object (never wiping
// dictionary/snippets/flowbar_position that aren't on the settings form).
let currentSettings = null;

// --- Tabs ---

function showTab(name) {
  $$(".tab").forEach((t) => t.classList.toggle("active", t.dataset.tab === name));
  $$(".panel").forEach((p) => p.classList.toggle("active", p.dataset.panel === name));
  if (name === "home") refreshHome();
  if (name === "history") loadHistory();
  if (name === "dictionary") loadDictionary();
  if (name === "snippets") loadSnippets();
}

$$(".tab").forEach((tab) => tab.addEventListener("click", () => showTab(tab.dataset.tab)));

// Rust can ask us to jump to a tab (tray / Flow Bar context menu).
listen("hub-navigate", (e) => showTab(e.payload));

// --- Dictation pipeline ---

const statusIndicator = $("#status-indicator");
const statusText = $("#status-text");
const recordBtn = $("#record-btn");
const outputArea = $("#output-area");
const outputText = $("#output-text");

let isRecording = false;
let busy = false;
let lastResult = "";

function setStatus(state, text) {
  statusIndicator.className = state;
  statusText.textContent = text;
}

function showOutput(text) {
  lastResult = text;
  outputText.textContent = text;
  outputArea.classList.remove("hidden");
}

async function startRecording() {
  if (isRecording || busy) return;
  outputArea.classList.add("hidden");
  try {
    await invoke("start_recording");
    isRecording = true;
    recordBtn.classList.add("recording");
    setStatus("recording", "Listening…");
  } catch (err) {
    setStatus("idle", `Error: ${err}`);
    uiLog("error", `${err}`);
  }
}

async function stopRecording() {
  if (!isRecording || busy) return;
  isRecording = false;
  busy = true;
  recordBtn.classList.remove("recording");
  try {
    const { duration_ms, samples } = await invoke("stop_recording");
    if (!samples) {
      setStatus("idle", "No audio captured");
      return;
    }
    setStatus("processing", `Transcribing (${(duration_ms / 1000).toFixed(1)}s)…`);
    const raw = await invoke("transcribe");
    if (!raw || raw.trim().length === 0) {
      setStatus("idle", "No speech detected");
      return;
    }
    setStatus("processing", "Cleaning up…");
    const cleaned = await invoke("cleanup");
    showOutput(cleaned);

    let copied = false;
    try {
      await invoke("copy_to_clipboard", { text: cleaned });
      copied = true;
    } catch (e) {
      uiLog("error", `clipboard failed: ${e}`);
    }
    setStatus("done", copied ? "Copied to clipboard" : "Done");
    if (currentSettings?.sound_enabled) playBeep();
  } catch (err) {
    setStatus("idle", `Error: ${err}`);
    uiLog("error", `${err}`);
  } finally {
    busy = false;
  }
}

recordBtn.addEventListener("click", () => {
  if (isRecording) stopRecording();
  else startRecording();
});

$("#copy-btn").addEventListener("click", async () => {
  try {
    await invoke("copy_to_clipboard", { text: lastResult });
    setStatus("done", "Copied");
  } catch (err) {
    uiLog("error", `${err}`);
  }
});

// A dictation completing anywhere (Flow Bar / hotkey) updates this tab too.
listen("dictation-complete", (e) => {
  if (busy) return;
  showOutput(e.payload);
  setStatus("done", "Done");
});

// --- Updates ---

let pendingUpdate = null;

function showUpdate(info) {
  pendingUpdate = info;
  $("#update-title").textContent = `Voxable ${info.version} is available`;
  // Release notes are markdown. Prefer the bullet points — those are the changes —
  // over the opening paragraph, which describes the app rather than what is new.
  const lines = (info.notes || "").split("\n");
  const bullets = lines
    .filter((l) => /^\s*[-*]\s+/.test(l))
    .map((l) =>
      l
        .replace(/^\s*[-*]\s+/, "")
        .replace(/\*\*/g, "")
        .replace(/`/g, "")
        .trim()
    )
    .filter(Boolean);
  const summary = bullets.length
    ? bullets.slice(0, 3).join(" · ")
    : lines.find((l) => l.trim() && !l.startsWith("#"))?.trim() || "";
  $("#update-notes").textContent = summary;
  $("#update-banner").hidden = false;
}

function showUpdateMessage(text) {
  pendingUpdate = null;
  $("#update-title").textContent = text;
  $("#update-notes").textContent = "";
  $("#update-download").hidden = true;
  $("#update-banner").hidden = false;
}

listen("update-available", (e) => {
  $("#update-download").hidden = false;
  showUpdate(e.payload);
});

// The launch check can finish before this window is listening, so ask for whatever
// it found rather than relying on having caught the event.
invoke("pending_update")
  .then((info) => {
    if (info) {
      $("#update-download").hidden = false;
      showUpdate(info);
    }
  })
  .catch((e) => uiLog("error", `could not read the pending update: ${e}`));
listen("update-none", () => showUpdateMessage("Voxable is up to date."));
listen("update-error", (e) => showUpdateMessage(`Could not check for updates: ${e.payload}`));

$("#update-download")?.addEventListener("click", () => {
  if (!pendingUpdate) return;
  invoke("open_release_page", { url: pendingUpdate.url }).catch((err) =>
    uiLog("error", `could not open the release page: ${err}`)
  );
});

$("#update-dismiss")?.addEventListener("click", () => {
  $("#update-banner").hidden = true;
});

// --- Settings ---

function getPresetFromUrl(url) {
  if (!url) return "custom";
  const presets = {
    openai: "api.openai.com",
    deepseek: "api.deepseek.com",
    groq: "api.groq.com",
    ollama: "localhost:11434",
    lmstudio: "localhost:1234",
  };
  for (const [preset, host] of Object.entries(presets)) {
    if (url.includes(host)) return preset;
  }
  return "custom";
}

function applyPreset(preset) {
  const presets = {
    openai: { url: "https://api.openai.com/v1", model: "gpt-4o-mini" },
    deepseek: { url: "https://api.deepseek.com/v1", model: "deepseek-chat" },
    groq: { url: "https://api.groq.com/openai/v1", model: "llama-3.1-8b-instant" },
    ollama: { url: "http://localhost:11434/v1", model: "llama3.2" },
    lmstudio: { url: "http://localhost:1234/v1", model: "local-model" },
    custom: { url: "", model: "" },
  };
  const p = presets[preset] || presets.custom;
  $("#llm-base-url").value = p.url;
  if (p.model) $("#llm-model").value = p.model;
}

$("#llm-preset").addEventListener("change", (e) => applyPreset(e.target.value));

function populateSettings(s) {
  $("#model-select").value = s.whisper_model || "base";
  $("#cleanup-level").value = s.cleanup_level || "medium";
  $("#llm-preset").value = getPresetFromUrl(s.llm_base_url);
  $("#llm-base-url").value = s.llm_base_url || "";
  $("#llm-api-key").value = s.llm_api_key || "";
  $("#llm-model").value = s.llm_model || "";
  setPendingHotkey(s.hotkey || DEFAULT_HOTKEY);
  $("#language-select").value = s.language || "en";
  $("#auto-paste").checked = s.auto_paste !== false;
  $("#sound-enabled").checked = s.sound_enabled !== false;
  $("#flowbar-hide-when-idle").checked = s.flowbar_hide_when_idle === true;
  $("#custom-prompt").value = s.custom_prompt || "";
}

$("#save-settings").addEventListener("click", async () => {
  // Spread the full current settings, override only the form fields — this is
  // what keeps dictionary / snippets / flowbar_position from being wiped.
  const merged = {
    ...currentSettings,
    whisper_model: $("#model-select").value,
    cleanup_level: $("#cleanup-level").value,
    llm_base_url: $("#llm-base-url").value.trim(),
    llm_api_key: $("#llm-api-key").value.trim(),
    llm_model: $("#llm-model").value.trim(),
    hotkey: pendingHotkey || DEFAULT_HOTKEY,
    language: $("#language-select").value,
    auto_paste: $("#auto-paste").checked,
    sound_enabled: $("#sound-enabled").checked,
    flowbar_hide_when_idle: $("#flowbar-hide-when-idle").checked,
    custom_prompt: $("#custom-prompt").value.trim(),
  };
  try {
    await invoke("save_settings", { settings: merged });
    // Re-register the hotkey in case it changed.
    try {
      await invoke("set_hotkey", { hotkey: merged.hotkey });
    } catch (e) {
      uiLog("error", `hotkey registration: ${e}`);
    }
    currentSettings = merged;
    setStatus("done", "Settings saved");
    refreshModelStatus();
  } catch (err) {
    alert(`Failed to save settings: ${err}`);
  }
});


// --- Hotkey recorder -------------------------------------------------------

// What the Settings tab will save. Kept separate from currentSettings so an
// unsaved capture can be abandoned by switching tabs.
let pendingHotkey = DEFAULT_HOTKEY;
let recording = false;

function setPendingHotkey(hotkey) {
  pendingHotkey = hotkey;
  renderHotkey($("#hotkey-keys"), hotkey);
  paintHotkeyHints(hotkey);
}

/** Show the current hotkey everywhere it appears outside Settings. */
function paintHotkeyHints(hotkey) {
  const hint = $("#hotkey-hint");
  if (hint) {
    hint.replaceChildren();
    const keys = document.createElement("span");
    renderHotkey(keys, hotkey);
    hint.append(keys, " to toggle from anywhere");
  }
  const home = $("#home-hotkey");
  if (home) {
    home.replaceChildren();
    const keys = document.createElement("span");
    renderHotkey(keys, hotkey);
    home.append("Press ", keys, " anywhere to dictate");
  }
}

async function checkHotkeyAvailable(hotkey) {
  const status = $("#hotkey-status");
  try {
    const free = await invoke("hotkey_available", { hotkey });
    const shown = hotkeyParts(hotkey).join(IS_MAC ? "" : "+");
    if (free) {
      status.textContent = `${shown} is available. Save settings to apply.`;
      status.classList.remove("warn");
    } else {
      status.textContent = `${shown} is taken — another app or macOS is already using it.`;
      status.classList.add("warn");
    }
    return free;
  } catch (e) {
    status.textContent = `Could not check that combination: ${e}`;
    status.classList.add("warn");
    return false;
  }
}

function stopRecordingHotkey() {
  recording = false;
  $("#hotkey-record").classList.remove("recording");
  window.removeEventListener("keydown", onHotkeyKeydown, true);
}

function onHotkeyKeydown(event) {
  event.preventDefault();
  event.stopPropagation();

  // A combination macOS claims first never reaches the webview at all, which is
  // indistinguishable from a broken recorder. Log every key we do see.
  uiLog(
    "info",
    `hotkey recorder saw code=${event.code} ctrl=${event.ctrlKey} alt=${event.altKey} ` +
      `shift=${event.shiftKey} meta=${event.metaKey}`
  );

  if (event.code === "Escape") {
    stopRecordingHotkey();
    setPendingHotkey(currentSettings?.hotkey || DEFAULT_HOTKEY);
    $("#hotkey-status").textContent = "Kept the previous hotkey.";
    return;
  }

  const hotkey = hotkeyFromEvent(event);
  if (!hotkey) {
    // Still holding modifiers — wait for the key that completes the combo.
    $("#hotkey-status").textContent = "Keep holding, then press a key…";
    return;
  }
  stopRecordingHotkey();
  setPendingHotkey(hotkey);
  checkHotkeyAvailable(hotkey);
}

function startRecordingHotkey() {
  if (recording) return;
  recording = true;
  $("#hotkey-record").classList.add("recording");
  $("#hotkey-status").textContent = IS_MAC
    ? "Press the keys you want. Esc cancels. (fn is set with Reset to default.)"
    : "Press the keys you want. Esc cancels.";
  window.addEventListener("keydown", onHotkeyKeydown, true);
}

// Focusing the display arms it — it looks like a field, so pressing keys should
// just work. Clicking focuses too, so this covers both.
$("#hotkey-record")?.addEventListener("focus", startRecordingHotkey);
$("#hotkey-record")?.addEventListener("click", () => {
  if (!recording) startRecordingHotkey();
});
$("#hotkey-record")?.addEventListener("blur", () => {
  if (recording) {
    stopRecordingHotkey();
    $("#hotkey-status").textContent = "Stopped listening.";
  }
});

$("#hotkey-reset")?.addEventListener("click", () => {
  stopRecordingHotkey();
  setPendingHotkey(DEFAULT_HOTKEY);
  $("#hotkey-status").textContent = IS_MAC
    ? "Back to the fn key. Save settings to apply."
    : "Back to the default. Save settings to apply.";
  $("#hotkey-status").classList.remove("warn");
});

// --- Home screen -----------------------------------------------------------

function formatDuration(minutes) {
  if (minutes <= 0) return "0m";
  if (minutes < 1) return "< 1m";
  const hours = Math.floor(minutes / 60);
  const mins = Math.round(minutes % 60);
  return hours ? `${hours}h ${mins}m` : `${mins}m`;
}

function formatCount(n) {
  return n.toLocaleString();
}

function relativeTime(iso) {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const seconds = Math.max(0, (Date.now() - then) / 1000);
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

async function refreshHome() {
  try {
    const stats = await invoke("get_stats");
    $("#stat-words").textContent = formatCount(stats.words);
    $("#stat-words-week").textContent = stats.words_this_week
      ? `${formatCount(stats.words_this_week)} in the last 7 days`
      : "Nothing yet this week";
    $("#stat-saved").textContent = formatDuration(stats.minutes_saved);
    $("#stat-wpm").textContent = stats.words_per_minute
      ? Math.round(stats.words_per_minute)
      : "—";
    $("#stat-dictations").textContent = formatCount(stats.dictations);
    $("#stat-top-app").textContent = stats.top_app ? stats.top_app[0] : "—";
  } catch (e) {
    uiLog("error", `get_stats failed: ${e}`);
  }

  try {
    const entries = await invoke("get_history");
    const list = $("#recent-list");
    list.replaceChildren();
    if (!entries.length) {
      const empty = document.createElement("p");
      empty.className = "panel-hint";
      empty.textContent = "Your dictations will show up here.";
      list.append(empty);
      return;
    }
    for (const entry of entries.slice(0, 5)) {
      const row = document.createElement("div");
      row.className = "recent-row";

      const text = document.createElement("div");
      text.className = "recent-text";
      text.textContent = entry.cleaned;

      const meta = document.createElement("div");
      meta.className = "recent-meta";
      meta.textContent = [relativeTime(entry.timestamp), entry.app]
        .filter(Boolean)
        .join(" · ");

      row.append(text, meta);
      row.addEventListener("click", async () => {
        await invoke("copy_to_clipboard", { text: entry.cleaned });
        meta.textContent = "Copied";
      });
      list.append(row);
    }
  } catch (e) {
    uiLog("error", `recent history failed: ${e}`);
  }
}

$("#view-all-history")?.addEventListener("click", () => showTab("history"));

async function refreshModelStatus() {
  try {
    const status = await invoke("model_status");
    const txt = status.downloaded
      ? `Downloaded: ${status.model}`
      : "Not downloaded — will download on first use";
    $("#model-status").textContent = txt;
    $("#model-badge").textContent = `Whisper · ${status.model}`;
  } catch {
    /* ignore */
  }
}

// --- Dictionary ---

async function loadDictionary() {
  const list = $("#dict-list");
  try {
    const entries = await invoke("get_dictionary");
    list.innerHTML = "";
    if (!entries.length) {
      list.innerHTML = '<div class="empty">No dictionary entries yet</div>';
      return;
    }
    for (const e of entries) {
      const row = document.createElement("div");
      row.className = "crud-row";
      row.innerHTML = `<span class="crud-key"></span><span class="crud-arrow">→</span><span class="crud-val"></span>`;
      row.querySelector(".crud-key").textContent = e.word;
      row.querySelector(".crud-val").textContent = e.replacement;
      const del = document.createElement("button");
      del.className = "crud-del";
      del.textContent = "✕";
      del.addEventListener("click", async () => {
        await invoke("remove_dictionary_entry", { word: e.word });
        loadDictionary();
      });
      row.appendChild(del);
      list.appendChild(row);
    }
  } catch (err) {
    uiLog("error", `${err}`);
  }
}

$("#dict-add").addEventListener("click", async () => {
  const word = $("#dict-word").value.trim();
  const replacement = $("#dict-replacement").value.trim();
  if (!word) return;
  try {
    await invoke("add_dictionary_entry", { word, replacement });
    $("#dict-word").value = "";
    $("#dict-replacement").value = "";
    loadDictionary();
    // keep currentSettings in sync
    currentSettings = await invoke("get_settings");
  } catch (err) {
    alert(err);
  }
});

// --- Snippets ---

async function loadSnippets() {
  const list = $("#snip-list");
  try {
    const entries = await invoke("get_snippets");
    list.innerHTML = "";
    if (!entries.length) {
      list.innerHTML = '<div class="empty">No snippets yet</div>';
      return;
    }
    for (const e of entries) {
      const row = document.createElement("div");
      row.className = "crud-row";
      row.innerHTML = `<span class="crud-key"></span><span class="crud-arrow">→</span><span class="crud-val"></span>`;
      row.querySelector(".crud-key").textContent = e.trigger;
      row.querySelector(".crud-val").textContent = e.expansion;
      const del = document.createElement("button");
      del.className = "crud-del";
      del.textContent = "✕";
      del.addEventListener("click", async () => {
        await invoke("remove_snippet", { trigger: e.trigger });
        loadSnippets();
      });
      row.appendChild(del);
      list.appendChild(row);
    }
  } catch (err) {
    uiLog("error", `${err}`);
  }
}

$("#snip-add").addEventListener("click", async () => {
  const trigger = $("#snip-trigger").value.trim();
  const expansion = $("#snip-expansion").value.trim();
  if (!trigger) return;
  try {
    await invoke("add_snippet", { trigger, expansion });
    $("#snip-trigger").value = "";
    $("#snip-expansion").value = "";
    loadSnippets();
    currentSettings = await invoke("get_settings");
  } catch (err) {
    alert(err);
  }
});

// --- History ---

function fmtTime(ts) {
  const d = new Date(ts);
  if (isNaN(d)) return ts;
  return d.toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

async function loadHistory() {
  const list = $("#history-list");
  try {
    const entries = await invoke("get_history"); // newest first
    list.innerHTML = "";
    if (!entries.length) {
      list.innerHTML = '<div class="empty">No dictations yet</div>';
      return;
    }
    for (const e of entries) {
      const card = document.createElement("div");
      card.className = "history-card";

      const meta = document.createElement("div");
      meta.className = "history-meta";
      const appName = e.app ? ` · ${e.app}` : "";
      const secs = (e.duration_ms / 1000).toFixed(1);
      meta.textContent = `${fmtTime(e.timestamp)} · ${e.model} · ${e.cleanup_level} · ${secs}s${appName}`;

      const text = document.createElement("div");
      text.className = "history-text";
      text.textContent = e.cleaned || e.raw;

      const actions = document.createElement("div");
      actions.className = "history-actions";

      const playBtn = document.createElement("button");
      playBtn.textContent = "▶ Play";
      playBtn.addEventListener("click", () => playHistoryAudio(e.id, playBtn));

      const copyBtn = document.createElement("button");
      copyBtn.textContent = "Copy";
      copyBtn.addEventListener("click", () =>
        invoke("copy_to_clipboard", { text: e.cleaned || e.raw }).catch((e) => uiLog("error", `copy failed: ${e}`))
      );

      actions.appendChild(playBtn);
      actions.appendChild(copyBtn);
      card.appendChild(meta);
      card.appendChild(text);
      card.appendChild(actions);
      list.appendChild(card);
    }
  } catch (err) {
    uiLog("error", `${err}`);
  }
}

async function playHistoryAudio(id, btn) {
  try {
    const samples = await invoke("get_history_audio", { id });
    if (!samples || !samples.length) {
      btn.textContent = "No audio";
      return;
    }
    const ctx = audioContext();
    const buf = ctx.createBuffer(1, samples.length, 16000);
    buf.copyToChannel(Float32Array.from(samples), 0);
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.connect(ctx.destination);
    btn.textContent = "▶ Playing…";
    src.onended = () => (btn.textContent = "▶ Play");
    src.start();
  } catch (err) {
    uiLog("error", `${err}`);
    btn.textContent = "Error";
  }
}

$("#clear-history").addEventListener("click", async () => {
  if (!confirm("Delete all dictation history and audio?")) return;
  try {
    await invoke("clear_history");
    loadHistory();
  } catch (err) {
    uiLog("error", `${err}`);
  }
});

// --- Init ---

(async () => {
  try {
    currentSettings = await invoke("get_settings");
    populateSettings(currentSettings);
  } catch (e) {
    uiLog("error", `${e}`);
  }
  refreshModelStatus();
  refreshHome();
})();

// A dictation finishing anywhere changes the numbers on the home screen.
listen("dictation-complete", () => refreshHome());
