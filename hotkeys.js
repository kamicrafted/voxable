// Hotkey capture and display.
//
// Two jobs: turn a real keypress into the string the Rust side registers, and show
// any hotkey the way the platform writes it (⌘⌥⌃⇧ on a Mac, Ctrl+Alt+… elsewhere).

export const IS_MAC = navigator.platform.toLowerCase().includes("mac");

export const DEFAULT_HOTKEY = IS_MAC ? "Fn" : "Ctrl+Alt+Space";

// How each modifier is written in a hotkey string, and how it is shown to a person.
// "Super" is Tauri's name for Command / the Windows key — it is not a name anyone
// uses out loud, so it never reaches the UI.
const MODIFIER_LABELS = IS_MAC
  ? { Control: "⌃", Alt: "⌥", Shift: "⇧", Super: "⌘", Fn: "fn" }
  : { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Super: "Win", Fn: "Fn" };

const KEY_LABELS = IS_MAC
  ? { Space: "Space", Enter: "↩", Backspace: "⌫", Escape: "⎋", Tab: "⇥" }
  : { Space: "Space", Enter: "Enter", Backspace: "Backspace", Escape: "Esc", Tab: "Tab" };

/** Split a hotkey string into the parts a person reads, e.g. ["⌘", "⇧", "D"]. */
export function hotkeyParts(hotkey) {
  if (!hotkey) return [];
  return hotkey.split("+").map((part) => {
    const key = part.trim();
    if (MODIFIER_LABELS[key]) return MODIFIER_LABELS[key];
    // Tauri spells the letter keys "KeyD" and the digits "Digit4".
    const named = key.replace(/^Key/, "").replace(/^Digit/, "");
    return KEY_LABELS[named] || named;
  });
}

/** Render a hotkey into an element as <kbd> chips. */
export function renderHotkey(el, hotkey) {
  el.replaceChildren();
  const parts = hotkeyParts(hotkey);
  parts.forEach((part, i) => {
    if (i > 0 && !IS_MAC) el.append("+");
    const kbd = document.createElement("kbd");
    kbd.textContent = part;
    el.append(kbd);
  });
  if (!parts.length) el.textContent = "Not set";
}

/**
 * Turn a keydown into a hotkey string, or null if it isn't a usable combo yet.
 *
 * A bare modifier is not a shortcut the OS can register, so holding ⌘ alone returns
 * null and the recorder keeps waiting. Fn is the exception, and it never reaches
 * keydown at all — the browser cannot see it, so it is offered as its own choice.
 */
export function hotkeyFromEvent(event) {
  const mods = [];
  if (event.ctrlKey) mods.push("Control");
  if (event.altKey) mods.push("Alt");
  if (event.shiftKey) mods.push("Shift");
  if (event.metaKey) mods.push("Super");

  const code = event.code;
  const isModifierItself =
    /^(Control|Alt|Shift|Meta)(Left|Right)$/.test(code) || code === "Fn";
  if (isModifierItself) return null;

  let key;
  if (/^Key[A-Z]$/.test(code)) key = code;
  else if (/^Digit[0-9]$/.test(code)) key = code;
  else if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) key = code;
  else if (code === "Space") key = "Space";
  else if (["Enter", "Backspace", "Escape", "Tab"].includes(code)) key = code;
  else if (/^Arrow(Up|Down|Left|Right)$/.test(code)) key = code;
  else return null;

  // Function keys can stand alone; everything else needs at least one modifier, or
  // the shortcut would swallow ordinary typing.
  if (!mods.length && !/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return null;

  return [...mods, key].join("+");
}
