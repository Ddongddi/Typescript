"use strict";
// 키 tab: a drawn keyboard to pick keys on, and the three roles (한영키, 언어 전환, 일시정지) with the
// keys each one has. Everything is saved through /api/settings, which checks the rules.

const ROLES = [
  { id: "hangulKey", name: "한영키", dot: "한", cls: "r-han", single: true,
    desc: "한영키로 쓸 키입니다. 아래 두 역할의 '한영키'가 이 키를 따라갑니다." },
  { id: "flipKeys", name: "언어 전환", dot: "전", cls: "r-flip",
    desc: "쓰는 중인(방금 친) 단어를 다음 언어로 바꿉니다. Esc는 기존 그대로 두거나 지워도 됩니다." },
  { id: "pauseKeys", name: "일시정지", dot: "정", cls: "r-pause",
    desc: "누르면 변환을 잠시 멈추고 쓴 그대로 입력합니다. 모두 지우면 일시정지를 쓰지 않습니다." },
];
const KEY_LIMIT = 8;
const MODS = ["Ctrl", "Alt", "Shift"];
const SIDES = { Ctrl: ["Ctrl", "RCtrl"], Alt: ["Alt", "RAlt"], Shift: ["Shift", "RShift"] };
const MOD_CODES = { ControlLeft: "Ctrl", ControlRight: "RCtrl", AltLeft: "Alt", AltRight: "RAlt", ShiftLeft: "Shift", ShiftRight: "RShift" };
const isModifier = (name) => /^R?(Ctrl|Alt|Shift)$/.test(name);

let keysUi = { role: "flipKeys", mods: { Ctrl: false, Alt: false, Shift: false } };
let capture = null;   // { solo } while a real key press is being waited for

// ---- the keyboard drawing ----------------------------------------------------------------

const L = (n, l, w, extra) => ({ n, l, w: w || 1, ...extra });
const D = (l, w, why) => ({ n: null, l, w: w || 1, why });
const G = (gap) => ({ gap });
const run = (from, to) => Array.from({ length: to - from + 1 }, (_, i) => L("F" + (from + i), "F" + (from + i)));
const letters = (s) => s.split("").map((c) => L(c, c));
const WIN = "Win 키는 Windows가 먼저 처리해서 입력기에 오지 않습니다";

const KEYBOARD = [
  [L("Esc", "Esc"), G(0.5), ...run(1, 4), G(0.25), ...run(5, 8), G(0.25), ...run(9, 12), G(0.25), L("Pause", "Pause"), L("ScrollLock", "ScrLk")],
  [D("`"), ...letters("1234567890"), D("-"), D("="), D("⌫", 2, "Backspace는 지정할 수 없습니다")],
  [D("Tab", 1.5, "Tab은 지정할 수 없습니다"), ...letters("QWERTYUIOP"), D("["), D("]"), D("\\", 1.5)],
  [L("CapsLock", "Caps", 1.75), ...letters("ASDFGHJKL"), D(";"), D("'"), D("Enter", 2.25, "Enter는 지정할 수 없습니다")],
  [L("Shift", "Shift", 2.25), ...letters("ZXCVBNM"), D(","), D("."), D("/"), L("RShift", "Shift", 2.75, { s: "오른쪽" })],
  [L("Ctrl", "Ctrl", 1.25), D("Win", 1.25, WIN), L("Alt", "Alt", 1.25), L("Space", "Space", 6.25), L("Hangul", "한/영", 1.25), L("Hanja", "한자", 1.25), L("RAlt", "Alt", 1.25, { s: "오른쪽" }), L("RCtrl", "Ctrl", 1.25, { s: "오른쪽" })],
  [...run(13, 24)],
];

// "Alt+HangulKey" -> { mods: Set, key } with the 한영키 role replaced by the key it stands for.
function parseChord(text, hangul) {
  const parts = text.split("+").map((p) => p.trim());
  let key = parts.pop();
  const mods = new Set(parts);
  if (key === "HangulKey" && hangul) {
    const h = parseChord(hangul, null);
    h.mods.forEach((m) => mods.add(m));
    key = h.key;
  }
  return { mods, key };
}

const rolesKeys = (role) => (role.single ? [state.settings.hangulKey] : state.settings[role.id]);
const keyHits = (chordKey, cap) => chordKey === cap || (SIDES[chordKey] || []).includes(cap);
const roleOf = (id) => ROLES.find((r) => r.id === id);

// Which roles use this cap: as the key itself, or as a modifier held with it.
function marksOf(cap) {
  const out = [];
  for (const role of ROLES) {
    let key = false, mod = false;
    for (const text of rolesKeys(role)) {
      const c = parseChord(text, state.settings.hangulKey);
      if (keyHits(c.key, cap)) key = true;
      for (const m of c.mods) if ((SIDES[m] || []).includes(cap)) mod = true;
    }
    if (key) out.push([role, "key"]);
    else if (mod) out.push([role, "mod"]);
  }
  return out;
}

function drawCap(item) {
  if (item.gap !== undefined) return h("span", { class: "gap", style: "--w:" + item.gap });
  const face = h("span", { class: "top" }, h("span", { class: "legend" }, item.l), item.s ? h("small", {}, item.s) : null);
  const style = "--w:" + item.w;
  if (!item.n) return h("div", { class: "cap dead", style, title: item.why || "지정할 수 없는 키입니다" }, face);
  const marks = marksOf(item.n);
  const badges = marks.map(([role, kind]) => h("i", { class: "badge " + role.cls + (kind === "mod" ? " mod" : ""), "aria-hidden": "true" }, role.dot));
  const used = marks.map(([role, kind]) => role.name + (kind === "mod" ? " (조합)" : ""));
  const name = item.l + (item.s ? " " + item.s : "");
  return h("button", {
    type: "button", class: "cap" + (marks.length ? " on " + marks[0][0].cls : ""), style,
    "data-f": "cap-" + item.n, disabled: busy,
    "aria-label": name + (used.length ? " — " + used.join(", ") : ""),
    title: used.length ? name + " — " + used.join(", ") : name + ": " + roleOf(keysUi.role).name + "에 지정",
    onclick: () => pickKey(item.n),
  }, face, badges);
}

// ---- changing keys -----------------------------------------------------------------------

function saveKeys(body, label, undoBody) {
  act(label, "/api/settings", body, null, { path: "/api/settings", body: undoBody });
}

// The chosen role gets the key (with the modifiers switched on, unless it is a modifier itself).
function assignKey(roleId, chord) {
  keysUi.mods = { Ctrl: false, Alt: false, Shift: false };
  if (roleId === "hangulKey") {
    saveKeys({ hangulKey: chord }, "한영키를 바꿨습니다", { hangulKey: state.settings.hangulKey });
    return;
  }
  const list = state.settings[roleId];
  saveKeys({ [roleId]: [...list, chord] }, "추가했습니다", { [roleId]: list });
}

function pickKey(name) {
  const held = MODS.filter((m) => keysUi.mods[m]);
  assignKey(keysUi.role, (isModifier(name) ? [name] : [...held, name]).join("+"));
}

// event.code -> the name settings.json uses, or null for a key that can't be assigned.
function keyName(code) {
  const fixed = { Escape: "Esc", Lang1: "Hangul", Lang2: "Hanja", Space: "Space", CapsLock: "CapsLock", Pause: "Pause", ScrollLock: "ScrollLock" };
  if (fixed[code]) return fixed[code];
  let m;
  if ((m = /^F(\d{1,2})$/.exec(code))) return code;
  if ((m = /^Key([A-Z])$/.exec(code))) return m[1];
  if ((m = /^Digit(\d)$/.exec(code))) return m[1];
  return null;
}

function stopCapture() {
  if (!capture) return;
  capture = null;
  document.removeEventListener("keydown", onCaptureDown, true);
  document.removeEventListener("keyup", onCaptureUp, true);
}

function finishCapture(chord) {
  stopCapture();
  assignKey(keysUi.role, chord);
}

function onCaptureDown(e) {
  if (!capture) return;
  e.preventDefault(); e.stopPropagation();
  if (e.repeat) return;
  if (MOD_CODES[e.code]) { capture.solo = MOD_CODES[e.code]; return; }
  capture.solo = null;
  if (e.code === "MetaLeft" || e.code === "MetaRight") { toast("Win 키는 Windows가 먼저 처리해서 쓸 수 없습니다", true); return; }
  const name = keyName(e.code);
  if (!name) { toast("이 키는 지정할 수 없습니다", true); return; }
  finishCapture([e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", name].filter(Boolean).join("+"));
}

function onCaptureUp(e) {
  if (!capture) return;
  e.preventDefault(); e.stopPropagation();
  if (MOD_CODES[e.code] && capture.solo === MOD_CODES[e.code]) finishCapture(capture.solo);
}

function startCapture() {
  capture = { solo: null };
  document.addEventListener("keydown", onCaptureDown, true);
  document.addEventListener("keyup", onCaptureUp, true);
  render("capture-cancel");
}

// ---- the tab -----------------------------------------------------------------------------

const KEY_LABELS = { HangulKey: "한영키", Hangul: "한/영", Hanja: "한자" };
const shown = (text) => text.split("+").map((p) => KEY_LABELS[p] || p).join(" + ");

function roleCard(role) {
  const keys = rolesKeys(role), defaults = role.single ? [state.defaults.hangulKey] : state.defaults[role.id];
  const changed = JSON.stringify(keys) !== JSON.stringify(defaults);
  const selected = keysUi.role === role.id;
  const chips = keys.map((k, i) => h("span", { class: "chip" }, h("kbd", {}, shown(k)),
    role.single ? null : h("button", {
      type: "button", class: "chip-x", "data-f": role.id + "-del-" + i, disabled: busy, "aria-label": shown(k) + " 지우기",
      onclick: () => saveKeys({ [role.id]: keys.filter((_, j) => j !== i) }, "지웠습니다", { [role.id]: keys }),
    }, "✕")));
  const reset = changed
    ? h("button", {
        type: "button", class: "btn quiet", "data-f": role.id + "-reset", disabled: busy,
        onclick: () => saveKeys({ [role.id]: role.single ? defaults[0] : defaults }, "기본값으로 되돌렸습니다", { [role.id]: role.single ? keys[0] : keys }),
      }, "기본값으로")
    : null;
  return h("div", { class: "role " + role.cls + (selected ? " picked" : "") },
    h("button", {
      type: "button", role: "radio", "aria-checked": selected, class: "role-pick", "data-f": "role-" + role.id,
      onclick: () => { keysUi.role = role.id; render("role-" + role.id); },
    }, h("i", { class: "badge " + role.cls, "aria-hidden": "true" }, role.dot), role.name,
      changed ? h("span", { class: "tag" }, "기본값 아님") : null),
    h("p", {}, role.desc),
    h("div", { class: "chips" }, chips,
      keys.length ? null : h("em", {}, "지정한 키 없음"),
      !role.single && keys.length >= KEY_LIMIT ? h("em", {}, "최대 " + KEY_LIMIT + "개") : null),
    reset);
}

function renderKeys() {
  const p = $("p-keys");
  const role = roleOf(keysUi.role);
  const mods = h("div", { class: "modbar", role: "group", "aria-label": "함께 누를 키" },
    h("span", {}, "함께 누르기"),
    MODS.map((m) => h("button", {
      type: "button", class: "pill", "aria-pressed": keysUi.mods[m], "data-f": "mod-" + m, disabled: busy,
      onclick: () => { keysUi.mods[m] = !keysUi.mods[m]; render("mod-" + m); },
    }, m)));
  const real = capture
    ? h("span", { class: "chip waiting", role: "status" }, "키를 누르세요…",
        h("button", { type: "button", class: "chip-x", "data-f": "capture-cancel", onclick: () => { stopCapture(); render("capture"); } }, "취소"))
    : h("button", { type: "button", class: "btn quiet", "data-f": "capture", disabled: busy, onclick: startCapture }, "실제 키 눌러 지정");
  p.replaceChildren(
    h("h2", {}, "키보드"),
    h("p", { class: "note" }, "아래에서 역할을 고른 뒤 키보드의 키를 클릭하거나 실제 키를 누르면 지정됩니다. 글자·숫자·Space는 Ctrl이나 Alt와 함께만 쓸 수 있고, Ctrl·Alt·Shift는 다른 키 없이 눌렀다 뗄 때 동작합니다."),
    h("div", { class: "pickbar" },
      h("div", { class: "picking" }, "지정할 역할 ", h("b", { class: role.cls }, role.name)), mods, real),
    h("div", { class: "kb", role: "group", "aria-label": "키보드" },
      KEYBOARD.map((row) => h("div", { class: "kb-row" }, row.map(drawCap)))),
    h("div", { class: "legend-row" }, ROLES.map((r) => h("span", {}, h("i", { class: "badge " + r.cls, "aria-hidden": "true" }, r.dot), r.name)),
      h("span", { class: "dim" }, "흐린 키는 지정할 수 없습니다 · 점선 배지는 함께 누르는 키")),
    h("h2", {}, "역할"),
    h("div", { class: "roles", role: "radiogroup", "aria-label": "지정할 역할" }, ROLES.map(roleCard)));
}
