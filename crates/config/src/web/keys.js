"use strict";
// 키 tab: what the 한/영 key does: switch the word's language (the default), nothing, or send a
// shortcut (Ctrl+C, Win+L, ...) to the app. Saved through /api/settings, which checks it.
//
// The shortcut is designated in two slots, [modifiers] + [one key], filled one at a time:
// modifiers only through buttons, the key through a lone keypress or a preset. Pressing a chord
// like Win+L for real would lock the screen; the Win button plus a lone L is the same shortcut
// without the danger. Nothing is saved until 지정 is pressed.

const SEND_PRESETS = ["Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+Z", "Ctrl+Y", "Ctrl+A", "Ctrl+S", "Ctrl+Space", "Alt+Tab", "Win+D", "Win+L"];
const SEND_MODS = ["Ctrl", "Alt", "Shift", "Win"];
const KEY_LABELS = { Hangul: "한/영", Hanja: "한자", RAlt: "오른쪽 Alt", RCtrl: "오른쪽 Ctrl", RShift: "오른쪽 Shift" };
const HELD_CODES = ["ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight"];

let capture = false;   // a lone keypress is waited for (slot 2)
let sendDraft = false;   // "단축키 보내기" chosen, nothing picked yet (nothing saved)
let sendMods = { Ctrl: false, Alt: false, Shift: false, Win: false };   // slot 1: held with the key
let assignOpen = false;   // the two-slot designation panel is open
let sendKey = "";   // slot 2: the key picked in the panel ("" = none yet)

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

// Slot 2 is filled one key at a time, never as a chord: pressing Win+L together would lock the
// screen, but the Win button plus a lone L designates the same shortcut without the danger.
function onCaptureDown(e) {
  if (!capture) return;
  // What is not the shortcut keeps working while the panel listens: typing in the chat on the
  // right, moving about with Tab (Ctrl/Alt+Tab is a shortcut, Shift+Tab is moving back), and
  // the arrow keys on a tab.
  if (editable(e.target)) return;
  if (e.code === "Tab" && !e.ctrlKey && !e.altKey) return;
  if (e.target && e.target.getAttribute && e.target.getAttribute("role") === "tab") return;
  // A focused button keeps working: Enter/Space on 지정 or 취소 must not become the key.
  if ((e.code === "Enter" || e.code === "Space") && e.target && e.target.tagName === "BUTTON") return;
  e.preventDefault(); e.stopPropagation();
  if (e.repeat) return;
  if (e.code === "Escape") { stopAssign(); renderKeysPanel("send-assign"); return; }   // Esc backs out, it never becomes the key
  if (e.code === "MetaLeft" || e.code === "MetaRight" || e.metaKey) {
    toast("Win 키는 왼쪽 슬롯의 Win 버튼으로 골라주세요 (함께 누르면 화면이 잠길 수 있습니다)", true);
    return;
  }
  if (HELD_CODES.includes(e.code)) return;   // a lone modifier belongs to slot 1, not this one
  const key = sendKeyName(e.code);
  if (!key) { toast("이 키는 보낼 수 없습니다", true); return; }
  // Held Ctrl/Alt/Shift join slot 1 (sticky: the toggles show them, click to take back off).
  sendMods.Ctrl = sendMods.Ctrl || e.ctrlKey;
  sendMods.Alt = sendMods.Alt || e.altKey;
  sendMods.Shift = sendMods.Shift || e.shiftKey;
  sendKey = key;
  renderKeysPanel("assign-confirm");
}

function startCapture() {
  stopHangulCapture();
  if (capture) return;
  capture = true;
  document.addEventListener("keydown", onCaptureDown, true);
}

function stopAssign() {
  assignOpen = false;
  sendKey = "";
  stopCapture();
}

// "Ctrl+Shift" + key, in the order the server shows them ("" while slot 2 is empty).
function draftShortcut() {
  const held = [
    sendMods.Ctrl ? "Ctrl" : "",
    sendMods.Alt ? "Alt" : "",
    sendMods.Shift ? "Shift" : "",
    sendMods.Win ? "Win" : "",
  ].filter(Boolean);
  return sendKey ? [...held, sendKey].join("+") : "";
}

function fillDraft(text) {
  const parts = text.split("+");
  sendKey = parts.length && parts[parts.length - 1] ? parts[parts.length - 1] : "";
  sendMods = { Ctrl: false, Alt: false, Shift: false, Win: false };
  for (const p of parts.slice(0, -1)) {
    if (p === "Ctrl") sendMods.Ctrl = true;
    else if (p === "Alt") sendMods.Alt = true;
    else if (p === "Shift") sendMods.Shift = true;
    else if (p === "Win") sendMods.Win = true;
  }
}

// The panel starts from what is sent now, so changing one slot is one action.
function openAssign() {
  const action = state.settings.hangulAction;
  fillDraft(action === "flip" || action === "none" ? "" : action);
  assignOpen = true;
  startCapture();
  renderKeysPanel("assign-cancel");
}

function confirmAssign() {
  const chord = draftShortcut();
  if (!sendKey) return;
  stopAssign();
  setShortcut(chord);
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
  renderKeysPanel("hangul-cancel");
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
    stopAssign();
    sendDraft = v === "send";
    // Choosing "send" opens the designation panel at once: there is no
    // "nothing picked yet" stop in between.
    if (v === "send") openAssign();
    else saveKeys({ hangulAction: v }, "저장했습니다", { hangulAction: action });
  };
  const cards = h("div", { class: "acts", role: "radiogroup", "aria-label": "한/영 키를 누르면" },
    ACTIONS.map(([v, title, hint]) => h("button", {
      type: "button", class: "act", role: "radio", "aria-checked": v === shownKind, "data-f": "act-" + v, disabled: busy,
      onclick: () => pick(v),
    }, h("b", {}, title), h("span", {}, hint))));

  let send = null;
  if (shownKind === "send" || assignOpen) {
    const now = kind === "send" ? action : "";
    const current = h("div", { class: "now" }, h("span", { class: "lbl" }, "보내는 단축키"),
      now ? h("span", { class: "chip" }, keyCaps(now)) : h("em", {}, "아직 고르지 않았습니다"),
      assignOpen ? null
        : h("button", { type: "button", class: "btn", "data-f": "send-assign", disabled: busy, onclick: openAssign }, "단축키 지정"));
    let panel = null;
    if (assignOpen) {
      const draft = draftShortcut();
      const mods = h("div", { class: "mods", role: "group", "aria-label": "함께 누를 키" },
        SEND_MODS.map((m) => h("button", {
          type: "button", class: "mod", "aria-pressed": sendMods[m], "data-f": "mod-" + m, disabled: busy,
          onclick: () => { sendMods[m] = !sendMods[m]; renderKeysPanel("mod-" + m); },
        }, m)));
      const presets = h("div", { class: "presets", role: "group", "aria-label": "자주 쓰는 단축키" },
        SEND_PRESETS.map((v) => h("button", {
          type: "button", class: "preset", "aria-pressed": v === draft, "data-f": "preset-" + v, disabled: busy,
          onclick: () => { fillDraft(v); renderKeysPanel("assign-confirm"); },
        }, v)));
      panel = h("div", { class: "assign", role: "group", "aria-label": "단축키 지정" },
        h("div", { class: "slots" },
          h("div", { class: "slot" }, h("span", { class: "lbl" }, "함께 누를 키 (버튼)"), mods),
          h("span", { class: "plus", "aria-hidden": "true" }, "+"),
          h("div", { class: "slot" }, h("span", { class: "lbl" }, "보낼 키 (한 번 누르기)"),
            h("span", { class: "slotkey" }, sendKey ? keyCaps(sendKey)
              : h("span", { class: "chip waiting", role: "status" }, "키를 한 번 누르세요")))),
        h("p", { class: "hint" }, "키는 하나씩만 누르세요. Win+L처럼 함께 누르면 화면이 잠기는 조합도, 버튼과 한 키씩 나누면 안전하게 지정됩니다. (Esc: 취소)"),
        h("div", { class: "decided" }, h("span", { class: "lbl" }, "지정될 단축키"),
          draft ? h("span", { class: "chip" }, keyCaps(draft)) : h("em", {}, "미정")),
        h("span", { class: "lbl" }, "또는 골라서 채우기"), presets,
        h("div", { class: "actions" },
          h("button", { type: "button", class: "btn primary", "data-f": "assign-confirm", disabled: busy || !sendKey, onclick: confirmAssign }, "지정"),
          h("button", { type: "button", class: "btn quiet", "data-f": "assign-cancel", disabled: busy, onclick: () => { stopAssign(); renderKeysPanel("send-assign"); } }, "취소")));
    }
    send = h("div", { class: "send" }, current, panel);
  }

  $("p-keys").replaceChildren(
    h("div", { class: "hangul" },
      h("kbd", { class: "keycap big" }, KEY_LABELS[state.settings.hangulKey] || state.settings.hangulKey),
      h("div", { class: "ask" }, h("b", {}, "이 키를 누르면"), h("span", {}, ["flip", "none"].includes(state.settings.hangulAction) ? "텍스트를 입력하는 창에서 동작합니다" : "어디서든 동작합니다")),
      hangulCapture
        ? h("span", { class: "chip waiting", role: "status" }, "한/영 키를 누르세요…",
            h("button", { type: "button", class: "chip-x", "data-f": "hangul-cancel", onclick: () => { stopHangulCapture(); renderKeysPanel("hangul-capture"); } }, "취소"))
        : h("button", { type: "button", class: "btn quiet", "data-f": "hangul-capture", disabled: busy, onclick: startHangulCapture }, "다른 키로 인식될 때")),
    cards, send || "",   // (never null: replaceChildren would write it as the word "null")
    h("div", { class: "keys-foot" },
      h("p", {}, "한/영 키가 오른쪽 Alt로 들어오는 노트북이면 “다른 키로 인식될 때”를 눌러 한/영 키를 한 번 누르세요."),
      h("p", {}, "더 많은 키 매핑을 하고 싶다면 ",
        h("a", { href: "https://github.com/iamallyouneed/KeyZen", target: "_blank", rel: "noopener noreferrer" }, "KeyZen"),
        "을 확인하세요.")));
}
