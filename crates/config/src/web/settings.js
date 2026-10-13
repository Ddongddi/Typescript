// 입력 설정 tab: switches and one dropdown, each with what it does, an example, and for two of
// them a few frames of the real thing.

// [group, [[key, title, what it does, example, language it needs], ...]]
const SETTINGS = [
  ["기본", [
    ["returnConfirmsOnly", "Enter는 확정만", "밑줄 단어에서 Enter를 누르면 단어만 확정하고, 다음 Enter가 앱으로 갑니다.", "켬: dkssud ⏎ → 안녕 (줄바꿈 없음) ⏎ → 줄바꿈 · 끔: ⏎ 한 번에 확정 + 줄바꿈"],
    ["autocomplete", "Tab 자동완성과 단어 기록", "쓰는 중인 단어를 자주 쓴 단어로 완성하고, 확정한 단어를 셉니다.", "hel 까지 치고 Tab → hello"],
    ["pauseKey", "일시정지 키", "누르면 변환을 잠시 멈추고 쓴 그대로 입력합니다.", ""],
  ]],
  ["일본어", [
    ["liveConversion", "치는 동안 바로 변환", "Space 전에도 첫 후보를 보여 줍니다.", "kyouha → 今日は (Space 없이)", "ja"],
    ["fullWidthDigits", "일본어 뒤 숫자는 전각", "일본어 문장 뒤에 이어 치는 숫자를 전각으로 씁니다.", "３月 ０３－１２３４", "ja"],
    ["fullWidthSpace", "일본어 뒤 공백은 전각", "Shift+Space로 반대 폭을 씁니다.", "東京　大阪 (전각 공백)", "ja"],
  ]],
  ["업데이트", [["checkUpdates", "새 버전 확인", "대시보드나 명령을 실행할 때 하루에 한 번 새 릴리스가 있는지 봅니다.", ""]]],
];

// Frames of a setting at work: the text in a field, whether it is still underlined (being
// typed), the key just pressed, and a word about what happened.
const DEMOS = {
  returnConfirmsOnly: (on) => [
    { text: "안녕", typing: true, note: "dkssud 를 쳤습니다" },
    on ? { text: "안녕", key: "Enter", note: "확정만 — 줄은 그대로" } : { text: "안녕↵", key: "Enter", note: "확정하고 줄바꿈" },
    ...(on ? [{ text: "안녕↵", key: "Enter", note: "한 번 더 눌러야 줄바꿈" }] : []),
  ],
  autocomplete: (on) => [
    { text: "hel", typing: true, note: "hel 까지 쳤습니다" },
    on ? { text: "hello", key: "Tab", note: "자주 쓴 단어로 완성" } : { text: "hel", key: "Tab", note: "자동완성이 꺼져 있어 그대로" },
  ],
};
let demoFor = null;   // the setting just changed: its demo plays by itself

function demo(key, on) {
  const frames = DEMOS[key](on);
  const field = h("span", { class: "field" });
  const hit = h("kbd", {}, "");
  const note = h("span", { class: "note" });
  const box = h("div", { class: "demo", "aria-hidden": "true" }, field, hit, note);
  let run = 0;
  const show = (f) => {
    field.replaceChildren(f.typing ? h("u", {}, f.text) : f.text);
    hit.textContent = f.key || "";
    hit.hidden = !f.key;
    hit.classList.toggle("hit", !!f.key);
    note.textContent = f.note;
  };
  const play = async () => {
    const mine = ++run;
    for (const f of frames) {
      if (mine !== run || !box.isConnected) return;
      show(f);
      await new Promise((done) => setTimeout(done, 900));
    }
  };
  show(frames[frames.length - 1]);
  box.append(h("button", { class: "btn quiet", type: "button", tabindex: "-1", onclick: play }, "▶ 다시"));
  // The page is drawn twice around a change; the demo plays in the last one (the first is busy).
  if (demoFor === key && !busy) { demoFor = null; queueMicrotask(play); }   // after it is in the page
  return box;
}

function setting(key, title, desc, example) {
  const value = state.settings[key], changed = value !== state.defaults[key];
  const save = (v, label) => {
    demoFor = key;
    act(label, "/api/settings", { [key]: v }, key, { path: "/api/settings", body: { [key]: value } });
  };
  const control = key === "pauseKey"
    ? dd({ key, label: title, value, options: [["hangul", "한/영 키"], ["off", "사용 안 함"]], onchange: (v) => save(v, "저장했습니다") })
    : h("label", { class: "switch" },
        h("input", { type: "checkbox", "data-f": key, checked: value, "aria-label": title, onchange: (e) => save(e.target.checked, "저장했습니다") }), h("i"));
  const reset = changed
    ? h("button", { class: "btn quiet", "data-f": key + "-reset", disabled: busy, "aria-label": title + " 기본값으로", onclick: () => save(state.defaults[key], "기본값으로 되돌렸습니다") }, "기본값으로")
    : null;
  return h("div", { class: "setting" },
    h("div", { class: "text" },
      h("b", {}, title, changed ? h("span", { class: "tag" }, "기본값 아님") : null),
      desc ? h("span", {}, desc) : null,
      example ? h("em", {}, example) : null,
      DEMOS[key] ? demo(key, value) : null),
    reset, control);
}

function renderSettings() {
  const p = $("p-set");
  p.replaceChildren();
  for (const [group, items] of SETTINGS) {
    const shown = items.filter((it) => !it[4] || state.order.includes(it[4]));
    if (!shown.length) continue;
    p.append(h("h2", {}, group), h("div", { class: "card" }, shown.map((it) => setting(it[0], it[1], it[2], it[3]))));
  }
  p.append(h("p", { class: "note", style: "margin-top:18px" }, "바꾸면 바로 저장되고, 열려 있는 앱에는 곧 반영됩니다."));
}
