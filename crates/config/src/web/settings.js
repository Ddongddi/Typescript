// 입력 설정 tab: switches, each with what it does and an example. (The chat to try them in is
// the pane on the right, chat.js.)

// [group, [[key, title, what it does, example, language it needs], ...]]
const SETTINGS = [
  [
    "기본",
    [
      [
        "returnConfirmsOnly",
        "Enter는 확정만",
        "밑줄 단어에서 Enter를 누르면 단어만 확정하고, 다음 Enter가 앱으로 갑니다.",
        "켬: dkssud ⏎ → 안녕 (줄바꿈 없음) ⏎ → 줄바꿈 · 끔: ⏎ 한 번에 확정 + 줄바꿈",
      ],
      [
        "autocomplete",
        "Tab 자동완성과 단어 기록",
        "쓰는 중인 단어를 자주 쓴 단어로 완성하고, 확정한 단어를 셉니다.",
        "hel 까지 치고 Tab → hello",
      ],
    ],
  ],
  /*language:ja*/
  [
    "일본어",
    [
      [
        "liveConversion",
        "치는 동안 바로 변환",
        "Space 전에도 첫 후보를 보여 줍니다.",
        "kyouha → 今日は (Space 없이)",
        "ja",
      ],
      [
        "fullWidthDigits",
        "일본어 뒤 숫자는 전각",
        "일본어 문장 뒤에 이어 치는 숫자를 전각으로 씁니다.",
        "３月 ０３－１２３４",
        "ja",
      ],
      [
        "fullWidthSpace",
        "일본어 뒤 공백은 전각",
        "Shift+Space로 반대 폭을 씁니다.",
        "東京　大阪 (전각 공백)",
        "ja",
      ],
    ],
  ],
  /*language:/ja*/
  [
    "업데이트",
    [
      [
        "checkUpdates",
        "새 버전 확인",
        "대시보드나 명령을 실행할 때 하루에 한 번 새 릴리스가 있는지 봅니다.",
        "",
      ],
    ],
  ],
];

function setting(key, title, desc, example) {
  const value = state.settings[key],
    changed = value !== state.defaults[key];
  const save = (v, label) => {
    act(label, "/api/settings", { [key]: v }, key, {
      path: "/api/settings",
      body: { [key]: value },
    });
  };
  const control = h(
    "label",
    { class: "switch" },
    h("input", {
      type: "checkbox",
      "data-f": key,
      checked: value,
      disabled: busy,
      "aria-label": title,
      onchange: (e) => save(e.target.checked, "저장했습니다"),
    }),
    h("i"),
  );
  const reset = changed
    ? h(
        "button",
        {
          class: "btn quiet",
          "data-f": key + "-reset",
          disabled: busy,
          "aria-label": title + " 기본값으로",
          onclick: () => save(state.defaults[key], "기본값으로 되돌렸습니다"),
        },
        "기본값으로",
      )
    : null;
  return h(
    "div",
    { class: "setting" },
    h(
      "div",
      { class: "text" },
      h("b", {}, title, changed ? h("span", { class: "tag" }, "기본값 아님") : null),
      desc ? h("span", {}, desc) : null,
      example ? h("em", {}, example) : null,
    ),
    reset,
    control,
  );
}

function renderSettings() {
  const p = $("p-set");
  p.replaceChildren();
  for (const [group, items] of SETTINGS) {
    const shown = items.filter((it) => !it[4] || state.order.includes(it[4]));
    if (!shown.length) continue;
    p.append(
      h("h2", {}, group),
      h(
        "div",
        { class: "card" },
        shown.map((it) => setting(it[0], it[1], it[2], it[3])),
      ),
    );
  }
  p.append(...wordHistory());
  p.append(
    h(
      "p",
      { class: "note", style: "margin-top:18px" },
      "바꾸면 바로 저장되고, 열려 있는 앱에는 곧 반영됩니다.",
    ),
  );
}
