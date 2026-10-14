"use strict";
// 키 tab: the three roles (한영키, 언어 전환, 일시정지) and the keys each one has. A key is added by
// pressing it (hold Ctrl/Alt for a combination) or picked from a list; the server checks the
// rules and says why a key is refused.

const ROLES = [
  { id: "hangulKey", name: "한영키", dot: "한", cls: "r-han", single: true,
    desc: "한영키로 쓸 키입니다. 아래 두 역할의 '한영키'가 이 키를 따라갑니다. 한/영 키가 없거나 다른 키로 오는 키보드에서 바꿉니다." },
  { id: "flipKeys", name: "언어 전환", dot: "전", cls: "r-flip",
    desc: "쓰는 중인(방금 친) 단어를 다음 언어로 바꿉니다. Esc는 기존 그대로 두거나 지워도 됩니다." },
  { id: "pauseKeys", name: "일시정지", dot: "정", cls: "r-pause",
    desc: "누르면 변환을 잠시 멈추고 쓴 그대로 입력합니다. 모두 지우면 일시정지를 쓰지 않습니다." },
];
const KEY_LIMIT = 8;
const MOD_CODES = { ControlLeft: "Ctrl", ControlRight: "RCtrl", AltLeft: "Alt", AltRight: "RAlt", ShiftLeft: "Shift", ShiftRight: "RShift" };
// [value, label] for the list; the first two only make sense inside the two lists.
const ROLE_PICKS = [["HangulKey", "한영키"], ["Alt+HangulKey", "Alt + 한영키"]];
const PICKS = [
  ["Hangul", "한/영"], ["Hanja", "한자"], ["Esc", "Esc"], ["CapsLock", "CapsLock"],
  ["RAlt", "오른쪽 Alt (단독)"], ["RCtrl", "오른쪽 Ctrl (단독)"], ["RShift", "오른쪽 Shift (단독)"],
  ["Ctrl", "Ctrl (단독)"], ["Alt", "Alt (단독)"], ["Shift", "Shift (단독)"],
  ["Ctrl+Space", "Ctrl + Space"], ["Pause", "Pause"], ["ScrollLock", "ScrollLock"],
  ...Array.from({ length: 12 }, (_, i) => ["F" + (13 + i), "F" + (13 + i)]),
];

let capture = null;   // { role, solo } while a real key press is being waited for

const KEY_LABELS = { HangulKey: "한영키", Hangul: "한/영", Hanja: "한자" };
const shown = (text) => text.split("+").map((p) => KEY_LABELS[p] || p).join(" + ");
// One keycap per key of a combination: Alt + 한영키 is two caps.
const keyCaps = (text) => text.split("+").flatMap((p, i) => [
  i ? h("span", { class: "plus", "aria-hidden": "true" }, "+") : null,
  h("kbd", { class: "keycap" }, KEY_LABELS[p] || p),
]);
const rolesKeys = (role) => (role.single ? [state.settings.hangulKey] : state.settings[role.id]);
// "Alt+HangulKey" -> "Alt + 한/영" when the key follows the 한영키; null for the rest.
const followed = (text) => (text.includes("HangulKey") ? shown(text.replace("HangulKey", state.settings.hangulKey)) : null);

function saveKeys(body, label, undoBody) {
  act(label, "/api/settings", body, null, { path: "/api/settings", body: undoBody });
}

function assignKey(roleId, chord) {
  if (roleId === "hangulKey") {
    saveKeys({ hangulKey: chord }, "한영키를 바꿨습니다", { hangulKey: state.settings.hangulKey });
    return;
  }
  const list = state.settings[roleId];
  saveKeys({ [roleId]: [...list, chord] }, "추가했습니다", { [roleId]: list });
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
  const { role } = capture;
  stopCapture();
  assignKey(role, chord);
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

function startCapture(role) {
  capture = { role, solo: null };
  document.addEventListener("keydown", onCaptureDown, true);
  document.addEventListener("keyup", onCaptureUp, true);
  render("capture-cancel");
}

function picker(role) {
  const extra = role.single ? [] : ROLE_PICKS;
  return h("select", {
    class: "keypick", "data-f": role.id + "-pick", disabled: busy, "aria-label": role.name + " 키를 목록에서 고르기",
    onchange: (e) => { const v = e.target.value; e.target.value = ""; if (v) assignKey(role.id, v); },
  }, h("option", { value: "" }, "목록에서 고르기"), [...extra, ...PICKS].map(([v, l]) => h("option", { value: v }, l)));
}

function roleCard(role) {
  const keys = rolesKeys(role), defaults = role.single ? [state.defaults.hangulKey] : state.defaults[role.id];
  const changed = JSON.stringify(keys) !== JSON.stringify(defaults);
  const waiting = capture && capture.role === role.id;
  const chips = keys.map((k, i) => {
    const hint = followed(k);
    return h("span", { class: "chip" }, keyCaps(k),
      hint ? h("span", { class: "hint" }, "= " + hint) : null,
      role.single ? null : h("button", {
        type: "button", class: "chip-x", "data-f": role.id + "-del-" + i, disabled: busy, "aria-label": shown(k) + " 지우기",
        onclick: () => saveKeys({ [role.id]: keys.filter((_, j) => j !== i) }, "지웠습니다", { [role.id]: keys }),
      }, "✕"));
  });
  const full = !role.single && keys.length >= KEY_LIMIT;
  const add = waiting
    ? h("span", { class: "chip waiting", role: "status" }, "키를 누르세요… (Ctrl·Alt를 누른 채 누르면 조합)",
        h("button", { type: "button", class: "chip-x", "data-f": "capture-cancel", onclick: () => { stopCapture(); render(role.id + "-add"); } }, "취소"))
    : h("button", { type: "button", class: "btn", "data-f": role.id + "-add", disabled: busy || full, onclick: () => startCapture(role.id) },
        role.single ? "키 바꾸기" : "＋ 키 추가");
  const reset = changed
    ? h("button", {
        type: "button", class: "btn quiet", "data-f": role.id + "-reset", disabled: busy,
        onclick: () => saveKeys({ [role.id]: role.single ? defaults[0] : defaults }, "기본값으로 되돌렸습니다", { [role.id]: role.single ? keys[0] : keys }),
      }, "기본값으로")
    : null;
  return h("div", { class: "role " + role.cls },
    h("div", { class: "role-head" },
      h("i", { class: "badge", "aria-hidden": "true" }, role.dot), h("b", {}, role.name),
      changed ? h("span", { class: "tag" }, "기본값 아님") : null, reset),
    h("p", {}, role.desc),
    h("div", { class: "chips" }, chips, keys.length ? null : h("em", {}, "지정한 키 없음")),
    h("div", { class: "actions" }, add, waiting ? null : picker(role), full ? h("em", {}, "최대 " + KEY_LIMIT + "개") : null));
}

function renderKeys() {
  $("p-keys").replaceChildren(
    h("p", { class: "note" }, "키는 직접 눌러 지정하거나 목록에서 고릅니다. Ctrl·Alt·Shift는 다른 키 없이 눌렀다 뗄 때 동작하고, 글자·숫자·Space는 Ctrl이나 Alt와 함께만 쓸 수 있습니다. Win 키 조합과 Ctrl+C 같은 흔한 단축키는 쓸 수 없습니다."),
    h("div", { class: "roles" }, ROLES.map(roleCard)));
}
