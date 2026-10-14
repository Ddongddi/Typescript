"use strict";
// 키 tab: what the 한/영 key does: switch the word's language (the default), nothing, or send a
// shortcut (Ctrl+C, Win+L, ...) to the app. Saved through /api/settings, which checks it.

const SEND_PRESETS = ["Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+Z", "Ctrl+Y", "Ctrl+A", "Ctrl+Space", "Win+D", "Win+L"];
const SEND_MODS = ["Ctrl", "Alt", "Shift", "Win"];
const KEY_LABELS = { Hangul: "한/영", Hanja: "한자" };
const HELD_CODES = ["ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight"];

let capture = false;   // a shortcut is being waited for
let sendDraft = false;   // "단축키 보내기" chosen, nothing picked yet (nothing saved)
let sendMods = { Ctrl: false, Alt: false, Shift: false, Win: false };   // held with the next key

// One keycap per key of a combination: Ctrl + C is two caps.
const keyCaps = (text) => text.split("+").flatMap((p, i) => [
  i ? h("span", { class: "plus", "aria-hidden": "true" }, "+") : null,
  h("kbd", { class: "keycap" }, KEY_LABELS[p] || p),
]);

function saveKeys(body, label, undoBody) {
  act(label, "/api/settings", body, null, { path: "/api/settings", body: undoBody });
}

function setShortcut(chord) {
  sendDraft = false;
  sendMods = { Ctrl: false, Alt: false, Shift: false, Win: false };
  saveKeys({ hangulAction: chord }, "단축키를 지정했습니다", { hangulAction: state.settings.hangulAction });
}

// event.code -> a key a shortcut can press, or null.
function sendKeyName(code) {
  const fixed = { Escape: "Esc", Space: "Space", Enter: "Enter", NumpadEnter: "Enter", Tab: "Tab", Backspace: "Backspace", Delete: "Delete", Insert: "Insert", Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown", ArrowLeft: "Left", ArrowUp: "Up", ArrowRight: "Right", ArrowDown: "Down" };
  if (fixed[code]) return fixed[code];
  let m;
  if ((m = /^F(\d{1,2})$/.exec(code))) return code;
  if ((m = /^Key([A-Z])$/.exec(code))) return m[1];
  if ((m = /^Digit(\d)$/.exec(code))) return m[1];
  return null;
}

function stopCapture() {
  if (!capture) return;
  capture = false;
  document.removeEventListener("keydown", onCaptureDown, true);
}

// The key pressed, with the modifiers held and the ones switched on (Win can't be held here:
// the system takes it).
function onCaptureDown(e) {
  if (!capture) return;
  e.preventDefault(); e.stopPropagation();
  if (e.repeat || HELD_CODES.includes(e.code)) return;
  const key = sendKeyName(e.code);
  if (!key) { toast("이 키는 보낼 수 없습니다", true); return; }
  const held = [e.ctrlKey || sendMods.Ctrl ? "Ctrl" : "", e.altKey || sendMods.Alt ? "Alt" : "", e.shiftKey || sendMods.Shift ? "Shift" : "", sendMods.Win ? "Win" : ""];
  stopCapture();
  setShortcut([...held, key].filter(Boolean).join("+"));
}

function startCapture() {
  capture = true;
  document.addEventListener("keydown", onCaptureDown, true);
  render("capture-cancel");
}

// The Han/Eng key and what it does.
function renderKeys() {
  const action = state.settings.hangulAction;
  const kind = action === "flip" || action === "none" ? action : "send";
  const shownKind = sendDraft ? "send" : kind;
  const choose = (e) => {
    const v = e.target.value;
    sendDraft = v === "send";
    if (v === "send") render("hangul-action");
    else saveKeys({ hangulAction: v }, "저장했습니다", { hangulAction: action });
  };
  const select = h("select", { class: "keypick wide", "data-f": "hangul-action", disabled: busy, "aria-label": "한/영 키를 누르면", onchange: choose },
    [["flip", "단어 언어 전환"], ["none", "아무것도 안 함"], ["send", "단축키 보내기"]].map(([v, l]) => h("option", { value: v, selected: v === shownKind }, l)));
  const preset = h("select", {
    class: "keypick", "data-f": "send-preset", disabled: busy, "aria-label": "자주 쓰는 단축키",
    onchange: (e) => { const v = e.target.value; e.target.value = ""; if (v) setShortcut(v); },
  }, h("option", { value: "" }, "자주 쓰는 단축키"), SEND_PRESETS.map((v) => h("option", { value: v }, v)));
  const press = capture
    ? h("span", { class: "chip waiting", role: "status" }, "키를 누르세요…",
        h("button", { type: "button", class: "chip-x", "data-f": "capture-cancel", onclick: () => { stopCapture(); render("send-capture"); } }, "취소"))
    : h("button", { type: "button", class: "btn", "data-f": "send-capture", disabled: busy, onclick: startCapture }, "키 눌러 지정");
  const mods = h("span", { class: "mods", role: "group", "aria-label": "함께 누를 키" },
    SEND_MODS.map((m) => h("button", {
      type: "button", class: "pill", "aria-pressed": sendMods[m], "data-f": "mod-" + m, disabled: busy,
      onclick: () => { sendMods[m] = !sendMods[m]; render("mod-" + m); },
    }, m)));
  const send = shownKind === "send"
    ? h("div", { class: "send" },
        kind === "send" ? h("span", { class: "chip" }, keyCaps(action)) : h("em", {}, "보낼 단축키를 고르세요"),
        preset, mods, press)
    : null;
  $("p-keys").replaceChildren(
    h("div", { class: "hangul" },
      h("kbd", { class: "keycap big" }, KEY_LABELS[state.settings.hangulKey] || state.settings.hangulKey),
      h("span", { class: "to", "aria-hidden": "true" }, "→"), select, send),
    h("p", { class: "note" }, "한/영 키를 누르면 일어날 일을 고릅니다. 단축키를 보내면 한/영 키는 그 단축키를 앱에 보내고 단어 언어 전환은 하지 않습니다. Win은 키보드로 누를 수 없어서 버튼으로 함께 고릅니다. 텍스트를 입력하는 창에서 동작합니다."));
}
