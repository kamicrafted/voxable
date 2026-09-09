import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => Array.from(document.querySelectorAll(sel));

// Keep the last-loaded settings so saves send the FULL object (never wiping
// dictionary/snippets/flowbar_position that aren't on the settings form).
let currentSettings = null;

// --- Tabs ---

function showTab(name) {
  $$(".tab").forEach((t) => t.classList.toggle("active", t.dataset.tab === name));
  $$(".panel").forEach((p) => p.classList.toggle("active", p.dataset.panel === name));
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
    console.error(err);
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
      console.error("clipboard failed:", e);
    }
    setStatus("done", copied ? "Copied to clipboard" : "Done");
    if (currentSettings?.sound_enabled) playBeep();
  } catch (err) {
    setStatus("idle", `Error: ${err}`);
    console.error(err);
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
    console.error(err);
  }
});

// A dictation completing anywhere (Flow Bar / hotkey) updates this tab too.
listen("dictation-complete", (e) => {
  if (busy) return;
  showOutput(e.payload);
  setStatus("done", "Done");
});

// --- Sound ---

let audioCtx = null;
function ensureCtx() {
  if (!audioCtx) audioCtx = new (window.AudioContext || window.webkitAudioContext)();
  return audioCtx;
}
function playBeep() {
  try {
    const ctx = ensureCtx();
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.frequency.value = 880;
    gain.gain.value = 0.1;
    osc.start();
    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.15);
    osc.stop(ctx.currentTime + 0.15);
  } catch {
    /* ignore */
  }
}

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
  $("#hotkey-input").value = s.hotkey || "Super+Alt+Space";
  $("#language-select").value = s.language || "en";
  $("#auto-paste").checked = s.auto_paste !== false;
  $("#sound-enabled").checked = s.sound_enabled !== false;
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
    hotkey: $("#hotkey-input").value.trim() || "Super+Alt+Space",
    language: $("#language-select").value,
    auto_paste: $("#auto-paste").checked,
    sound_enabled: $("#sound-enabled").checked,
    custom_prompt: $("#custom-prompt").value.trim(),
  };
  try {
    await invoke("save_settings", { settings: merged });
    // Re-register the hotkey in case it changed.
    try {
      await invoke("set_hotkey", { hotkey: merged.hotkey });
    } catch (e) {
      console.error("hotkey registration:", e);
    }
    currentSettings = merged;
    setStatus("done", "Settings saved");
    refreshModelStatus();
  } catch (err) {
    alert(`Failed to save settings: ${err}`);
  }
});

async function refreshModelStatus() {
  try {
    const status = await invoke("model_status");
    const txt = status.downloaded
      ? `Downloaded: ${status.model}`
      : "Not downloaded — will download on first use";
    $("#model-status").textContent = txt;
    $("#model-badge").textContent = status.model;
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
    console.error(err);
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
    console.error(err);
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
        invoke("copy_to_clipboard", { text: e.cleaned || e.raw }).catch(console.error)
      );

      actions.appendChild(playBtn);
      actions.appendChild(copyBtn);
      card.appendChild(meta);
      card.appendChild(text);
      card.appendChild(actions);
      list.appendChild(card);
    }
  } catch (err) {
    console.error(err);
  }
}

async function playHistoryAudio(id, btn) {
  try {
    const samples = await invoke("get_history_audio", { id });
    if (!samples || !samples.length) {
      btn.textContent = "No audio";
      return;
    }
    const ctx = ensureCtx();
    const buf = ctx.createBuffer(1, samples.length, 16000);
    buf.copyToChannel(Float32Array.from(samples), 0);
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.connect(ctx.destination);
    btn.textContent = "▶ Playing…";
    src.onended = () => (btn.textContent = "▶ Play");
    src.start();
  } catch (err) {
    console.error(err);
    btn.textContent = "Error";
  }
}

$("#clear-history").addEventListener("click", async () => {
  if (!confirm("Delete all dictation history and audio?")) return;
  try {
    await invoke("clear_history");
    loadHistory();
  } catch (err) {
    console.error(err);
  }
});

// --- Init ---

(async () => {
  try {
    currentSettings = await invoke("get_settings");
    populateSettings(currentSettings);
  } catch (e) {
    console.error(e);
  }
  refreshModelStatus();
})();
