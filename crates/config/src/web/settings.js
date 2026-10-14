// 입력 설정 tab: a chat to try the settings in (chat.js), then switches and one dropdown, each with
// what it does and an example.

// [group, [[key, title, what it does, example, language it needs], ...]]
const SETTINGS = [
  ["기본", [
    ["returnConfirmsOnly", "Enter는 확정만", "밑줄 단어에서 Enter를 누르면 단어만 확정하고, 다음 Enter가 앱으로 갑니다.", "켬: dkssud ⏎ → 안녕 (줄바꿈 없음) ⏎ → 줄바꿈 · 끔: ⏎ 한 번에 확정 + 줄바꿈"],
    ["autocomplete", "Tab 자동완성과 단어 기록", "쓰는 중인 단어를 자주 쓴 단어로 완성하고, 확정한 단어를 셉니다.", "hel 까지 치고 Tab → hello"],
  ]],
  ["일본어", [
    ["liveConversion", "치는 동안 바로 변환", "Space 전에도 첫 후보를 보여 줍니다.", "kyouha → 今日は (Space 없이)", "ja"],
    ["fullWidthDigits", "일본어 뒤 숫자는 전각", "일본어 문장 뒤에 이어 치는 숫자를 전각으로 씁니다.", "３月 ０３－１２３４", "ja"],
    ["fullWidthSpace", "일본어 뒤 공백은 전각", "Shift+Space로 반대 폭을 씁니다.", "東京　大阪 (전각 공백)", "ja"],
  ]],
  ["업데이트", [["checkUpdates", "새 버전 확인", "대시보드나 명령을 실행할 때 하루에 한 번 새 릴리스가 있는지 봅니다.", ""]]],
];

function setting(key, title, desc, example) {
  const value = state.settings[key], changed = value !== state.defaults[key];
  const save = (v, label) => {
    act(label, "/api/settings", { [key]: v }, key, { path: "/api/settings", body: { [key]: value } });
  };
  const control = h("label", { class: "switch" },
    h("input", { type: "checkbox", "data-f": key, checked: value, "aria-label": title, onchange: (e) => save(e.target.checked, "저장했습니다") }), h("i"));
  const reset = changed
    ? h("button", { class: "btn quiet", "data-f": key + "-reset", disabled: busy, "aria-label": title + " 기본값으로", onclick: () => save(state.defaults[key], "기본값으로 되돌렸습니다") }, "기본값으로")
    : null;
  return h("div", { class: "setting" },
    h("div", { class: "text" },
      h("b", {}, title, changed ? h("span", { class: "tag" }, "기본값 아님") : null),
      desc ? h("span", {}, desc) : null,
      example ? h("em", {}, example) : null),
    reset, control);
}

// 키 지정: chips of the assigned keys; "＋ 키 추가" waits for the real key (combination).
const KEY_LIMIT = 8;
const KEY_ROWS = [
  ["flipKeys", "언어 전환 키", "쓰는 중인(방금 친) 단어를 다음 언어로 바꿉니다. Esc는 기존 그대로 두거나 지워도 됩니다."],
  ["pauseKeys", "일시정지 키", "누르면 변환을 잠시 멈추고 쓴 그대로 입력합니다. 키를 모두 지우면 일시정지를 쓰지 않습니다."],
];
const MOD_NAMES = { ControlLeft: "Ctrl", ControlRight: "RCtrl", AltLeft: "Alt", AltRight: "RAlt", ShiftLeft: "Shift", ShiftRight: "RShift" };
let capture = null;   // { key, solo } while a key is being waited for

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
  capture = null;
  document.removeEventListener("keydown", onCaptureDown, true);
  document.removeEventListener("keyup", onCaptureUp, true);
}

function finishCapture(chord) {
  const { key } = capture;
  stopCapture();
  const list = state.settings[key];
  act("추가했습니다", "/api/settings", { [key]: [...list, chord] }, key + "-add", { path: "/api/settings", body: { [key]: list } });
}

function onCaptureDown(e) {
  if (!capture) return;
  e.preventDefault(); e.stopPropagation();
  if (e.repeat) return;
  if (MOD_NAMES[e.code]) { capture.solo = MOD_NAMES[e.code]; return; }
  if (e.code === "MetaLeft" || e.code === "MetaRight") { toast("Win 키는 Windows가 먼저 처리해서 쓸 수 없습니다", true); return; }
  const name = keyName(e.code);
  if (!name) { toast("이 키는 지정할 수 없습니다", true); return; }
  const chord = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", name].filter(Boolean).join("+");
  finishCapture(chord);
}

function onCaptureUp(e) {
  if (!capture) return;
  e.preventDefault(); e.stopPropagation();
  if (MOD_NAMES[e.code] && capture.solo === MOD_NAMES[e.code]) finishCapture(capture.solo);
}

function startCapture(key) {
  capture = { key, solo: null };
  document.addEventListener("keydown", onCaptureDown, true);
  document.addEventListener("keyup", onCaptureUp, true);
  render(key + "-cancel");
}

function keyRow([key, title, desc]) {
  const list = state.settings[key], defaults = state.defaults[key];
  const changed = JSON.stringify(list) !== JSON.stringify(defaults);
  const set = (next, label) => act(label, "/api/settings", { [key]: next }, key + "-add", { path: "/api/settings", body: { [key]: list } });
  const waiting = capture && capture.key === key;
  const chips = list.map((k, i) => h("span", { class: "chip" },
    h("kbd", {}, k),
    h("button", { type: "button", class: "chip-x", "data-f": key + "-del-" + i, disabled: busy, "aria-label": k + " 지우기", onclick: () => set(list.filter((_, j) => j !== i), "지웠습니다") }, "✕")));
  const add = waiting
    ? h("span", { class: "chip waiting", role: "status" }, "키를 누르세요…",
        h("button", { type: "button", class: "chip-x", "data-f": key + "-cancel", "aria-label": "취소", onclick: () => { stopCapture(); render(key + "-add"); } }, "취소"))
    : h("button", { type: "button", class: "btn quiet", "data-f": key + "-add", disabled: busy || list.length >= KEY_LIMIT, onclick: () => startCapture(key) }, "＋ 키 추가");
  const reset = changed
    ? h("button", { type: "button", class: "btn quiet", "data-f": key + "-reset", disabled: busy, onclick: () => set(defaults, "기본값으로 되돌렸습니다") }, "기본값으로")
    : null;
  return h("div", { class: "setting keyrow" },
    h("div", { class: "text" },
      h("b", {}, title, changed ? h("span", { class: "tag" }, "기본값 아님") : null),
      h("span", {}, desc),
      h("div", { class: "chips" }, chips, add, list.length ? null : h("em", {}, "지정한 키 없음"))),
    reset);
}

function renderSettings() {
  const p = $("p-set");
  p.replaceChildren(h("h2", {}, "채팅으로 체험"), chatBox());
  for (const [group, items] of SETTINGS) {
    const shown = items.filter((it) => !it[4] || state.order.includes(it[4]));
    if (!shown.length) continue;
    p.append(h("h2", {}, group), h("div", { class: "card" }, shown.map((it) => setting(it[0], it[1], it[2], it[3]))));
  }
    p.append(h("h2", {}, "키 지정"), h("div", { class: "card" }, KEY_ROWS.map(keyRow)));
  p.append(h("p", { class: "note", style: "margin-top:18px" }, "바꾸면 바로 저장되고, 열려 있는 앱에는 곧 반영됩니다."));
}
