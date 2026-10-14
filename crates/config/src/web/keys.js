"use strict";
// 키 tab: what the 한/영 key does: switch the word's language (the default), nothing, or send a
// shortcut (Ctrl+C, Win+L, ...) to the app. Saved through /api/settings, which checks it.

const SEND_PRESETS = ["Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+Z", "Ctrl+Y", "Ctrl+A", "Ctrl+Space", "Win+D", "Win+L"];
const SEND_MODS = ["Ctrl", "Alt", "Shift", "Win"];
const KEY_LABELS = { Hangul: "한/영", Hanja: "한자", RAlt: "오른쪽 Alt", RCtrl: "오른쪽 Ctrl", RShift: "오른쪽 Shift" };
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
  stopHangulCapture();
  capture = true;
  document.addEventListener("keydown", onCaptureDown, true);
  render("capture-cancel");
}

// Which physical key is the Han/Eng key. Many laptops send it as the right Alt, not as Hangul:
// pressing it here finds out. event.code -> the name settings.json takes, or null.
let hangulCapture = false;
function hangulKeyName(code) {
  const fixed = { Lang1: "Hangul", HangulMode: "Hangul", Lang2: "Hanja", HanjaMode: "Hanja", AltRight: "RAlt", ControlRight: "RCtrl", ShiftRight: "RShift", CapsLock: "CapsLock", ScrollLock: "ScrollLock", Pause: "Pause" };
  if (fixed[code]) return fixed[code];
  return /^F(\d{1,2})$/.test(code) ? code : null;
}
function onHangulDown(e) {
  e.preventDefault(); e.stopPropagation();
  if (e.repeat) return;
  const name = hangulKeyName(e.code);
  if (!name) { toast("이 키는 한/영 키로 쓸 수 없습니다", true); return; }
  stopHangulCapture();
  saveKeys({ hangulKey: name }, "한/영 키를 " + (KEY_LABELS[name] || name) + "(으)로 지정했습니다", { hangulKey: state.settings.hangulKey, pauseKeys: state.settings.pauseKeys });
}
function stopHangulCapture() {
  if (!hangulCapture) return;
  hangulCapture = false;
  document.removeEventListener("keydown", onHangulDown, true);
}
function startHangulCapture() {
  stopCapture();
  hangulCapture = true;
  document.addEventListener("keydown", onHangulDown, true);
  render("hangul-cancel");
}

// The three things the Han/Eng key can do.
const ACTIONS = [
  ["flip", "단어 언어 전환", "누를 때마다 영·한·일을 돌려 가며 바꿉니다"],
  ["send", "단축키 보내기", "한/영 키 대신 고른 단축키를 앱에 보냅니다"],
  ["none", "아무것도 안 함", "눌러도 아무 일도 없습니다"],
];

// The Han/Eng key and what it does.
function renderKeys() {
  const action = state.settings.hangulAction;
  const kind = action === "flip" || action === "none" ? action : "send";
  const shownKind = sendDraft ? "send" : kind;
  const pick = (v) => {
    if (busy || v === shownKind) return;
    sendDraft = v === "send";
    if (v === "send") render("act-send");
    else saveKeys({ hangulAction: v }, "저장했습니다", { hangulAction: action });
  };
  const cards = h("div", { class: "acts", role: "radiogroup", "aria-label": "한/영 키를 누르면" },
    ACTIONS.map(([v, title, hint]) => h("button", {
      type: "button", class: "act", role: "radio", "aria-checked": v === shownKind, "data-f": "act-" + v, disabled: busy,
      onclick: () => pick(v),
    }, h("b", {}, title), h("span", {}, hint))));

  let send = null;
  if (shownKind === "send") {
    const now = kind === "send" ? action : "";
    const presets = h("div", { class: "presets", role: "group", "aria-label": "자주 쓰는 단축키" },
      SEND_PRESETS.map((v) => h("button", { type: "button", class: "preset", "aria-pressed": v === now, "data-f": "preset-" + v, disabled: busy, onclick: () => setShortcut(v) }, v)));
    const mods = h("div", { class: "mods", role: "group", "aria-label": "함께 누를 키" },
      SEND_MODS.map((m) => h("button", {
        type: "button", class: "mod", "aria-pressed": sendMods[m], "data-f": "mod-" + m, disabled: busy,
        onclick: () => { sendMods[m] = !sendMods[m]; render("mod-" + m); },
      }, m)));
    const press = capture
      ? h("span", { class: "chip waiting", role: "status" }, "키를 누르세요…",
          h("button", { type: "button", class: "chip-x", "data-f": "capture-cancel", onclick: () => { stopCapture(); render("send-capture"); } }, "취소"))
      : h("button", { type: "button", class: "btn", "data-f": "send-capture", disabled: busy, onclick: startCapture }, "키 눌러 지정");
    send = h("div", { class: "send" },
      h("div", { class: "now" }, h("span", { class: "lbl" }, "보내는 단축키"),
        now ? h("span", { class: "chip" }, keyCaps(now)) : h("em", {}, "아직 고르지 않았습니다")),
      h("div", { class: "own" }, press, h("span", { class: "lbl" }, "함께 누를 키"), mods),
      h("span", { class: "lbl" }, "또는 골라서 지정"), presets);
  }

  $("p-keys").replaceChildren(
    h("div", { class: "hangul" },
      h("kbd", { class: "keycap big" }, KEY_LABELS[state.settings.hangulKey] || state.settings.hangulKey),
      h("div", { class: "ask" }, h("b", {}, "이 키를 누르면"), h("span", {}, ["flip", "none"].includes(state.settings.hangulAction) ? "텍스트를 입력하는 창에서 동작합니다" : "어디서든 동작합니다")),
      hangulCapture
        ? h("span", { class: "chip waiting", role: "status" }, "한/영 키를 누르세요…",
            h("button", { type: "button", class: "chip-x", "data-f": "hangul-cancel", onclick: () => { stopHangulCapture(); render("hangul-capture"); } }, "취소"))
        : h("button", { type: "button", class: "btn quiet", "data-f": "hangul-capture", disabled: busy, onclick: startHangulCapture }, "다른 키로 인식될 때")),
    cards, send,
    h("div", { class: "keys-foot" },
      h("p", {}, "Win은 키보드로 누를 수 없어서 버튼으로 함께 고릅니다. 한/영 키가 오른쪽 Alt로 들어오는 노트북이면 “다른 키로 인식될 때”를 눌러 한/영 키를 한 번 누르세요."),
      h("p", {}, "더 많은 키 매핑을 하고 싶다면 ",
        h("a", { href: "https://github.com/iamallyouneed/KeyZen", target: "_blank", rel: "noopener noreferrer" }, "KeyZen"),
        "을 확인하세요.")));
}
