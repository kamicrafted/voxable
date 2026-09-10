// Dictation feedback tones, shared by the Flow Bar and the Hub.
//
// Synthesized rather than shipped as audio files: a few oscillator notes need no
// assets, no licensing, and no decode step, and they stay crisp at any sample rate.
//
// Two rules learned the hard way about short cues:
//  - Start an oscillator at full gain and you hear a click, not a note. Every tone
//    ramps up over a few milliseconds.
//  - These fire on every dictation, so they sit quieter than a one-off notification
//    beep. Loud enough to notice, quiet enough to stop noticing.

let ctx = null;

/// The shared AudioContext. Exported because history playback needs to build audio
/// buffers on it — one context per window, not one per feature.
export function audioContext() {
  return audio();
}

function audio() {
  if (!ctx) ctx = new (window.AudioContext || window.webkitAudioContext)();
  // Browsers suspend a context created before any user gesture; a dictation
  // triggered by a global hotkey is not a gesture as far as the webview knows.
  if (ctx.state === "suspended") ctx.resume();
  return ctx;
}

/// One short sine note. `at` is an offset in seconds from now, so notes can be
/// sequenced without timers.
function note(freq, { at = 0, ms = 60, gain = 0.06 } = {}) {
  const c = audio();
  const t0 = c.currentTime + at;
  const t1 = t0 + ms / 1000;

  const osc = c.createOscillator();
  const amp = c.createGain();
  osc.type = "sine";
  osc.frequency.value = freq;
  osc.connect(amp);
  amp.connect(c.destination);

  amp.gain.setValueAtTime(0.0001, t0);
  amp.gain.exponentialRampToValueAtTime(gain, t0 + 0.006);
  amp.gain.exponentialRampToValueAtTime(0.0001, t1);

  osc.start(t0);
  osc.stop(t1 + 0.02);
}

/// Recording started: two quick ascending notes, so it reads as "go".
export function playStart() {
  try {
    note(660, { ms: 55 });
    note(990, { at: 0.055, ms: 55 });
  } catch {
    /* audio is a nicety; never let it break dictation */
  }
}

/// Recording stopped: one lower note, so start and stop are never confusable.
export function playStop() {
  try {
    note(520, { ms: 90 });
  } catch {
    /* ignore */
  }
}

/// The Hub's completion beep. Unchanged in character from the original inline
/// version so it stays recognizable; kept here so there is one sound file.
export function playBeep() {
  try {
    note(880, { ms: 150, gain: 0.1 });
  } catch {
    /* ignore */
  }
}
