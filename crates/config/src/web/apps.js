// 앱별 규칙 tab: a card per program, the programs running now, and a way to add one by hand.

const TRI = [
  ["", "기본"],
  ["true", "켜기"],
  ["false", "끄기"],
];
const tri = (v) => (v === true ? "true" : v === false ? "false" : "");
const triVal = (s) => (s === "" ? undefined : s === "true");
const PRESETS = [
  ["터미널 · 엄격", { conversionThreshold: 0.97 }],
  ["게임 · 끄기", { enabled: false }],
  ["vim · 에디터", { escapeToApp: true }],
  ["카카오톡 · 시스템 IME", { useSystemIme: true }],
];
const PAGE_SIZE = 12;
const BLANK = "blank";
const DEFAULT_THRESHOLD = 0.5;

const running = { list: null, loading: false, filter: "", page: 0 };

const saveApps = (apps, key) =>
  act("저장했습니다", "/api/apps", apps, key, { path: "/api/apps", body: copy(state.apps) });

function summary(rule) {
  const out = [];
  if (rule.enabled === false) out.push("꺼짐");
  else if (rule.enabled === true) out.push("항상 켜짐");
  if (rule.conversionThreshold !== undefined)
    out.push(
      rule.conversionThreshold >= 0.97
        ? "엄격 " + rule.conversionThreshold
        : "기준 " + rule.conversionThreshold,
    );
  if (rule.escapeToApp !== undefined)
    out.push(rule.escapeToApp ? "Esc는 앱에게" : "Esc는 변환 전환");
  if (rule.useSystemIme !== undefined) out.push(rule.useSystemIme ? "시스템 IME" : "자체 입력기");
  if (rule.fullWidth !== undefined) out.push(rule.fullWidth ? "일본어 전각" : "전각 안 씀");
  return out;
}

// The words of a sample, as the input method would leave them for a given threshold.
function previewNodes(segments) {
  return segments.map((s) =>
    s.keys ? h("span", { class: (s.kept ? "kept " : "") + "is-" + s.lang }, s.out) : s.text,
  );
}

// Moving the slider shows how a sample reads at that threshold before anything is saved.
function thresholdField(exe, rule, edit) {
  const th = rule.conversionThreshold;
  const out = h("output", {}, th === undefined ? "기본" : th.toFixed(2));
  const preview = h("p", { class: "preview", "aria-live": "polite" }, " ");
  let latest = 0;
  const show = async (value) => {
    const mine = ++latest;
    try {
      const segments = await readTyping(PREVIEW_TEXT, value);
      if (mine === latest) preview.replaceChildren(...previewNodes(segments));
    } catch (e) {
      if (mine === latest) preview.textContent = e.message;
    }
  };
  const input = h("input", {
    type: "range",
    min: "0.5",
    max: "0.99",
    step: "0.01",
    value: String(th === undefined ? DEFAULT_THRESHOLD : th),
    "data-f": "app-" + exe + "-th",
    "aria-label": exe + " 변환 기준",
    disabled: busy,
    oninput: (e) => {
      out.textContent = Number(e.target.value).toFixed(2);
      show(Number(e.target.value));
    },
    onchange: (e) => edit({ conversionThreshold: Number(e.target.value) }, "app-" + exe + "-th"),
  });
  show(th === undefined ? appThreshold("*") : th);
  return h(
    "div",
    { class: "field wide" },
    h("span", {}, "변환 기준 — 높을수록 확실할 때만 바꾸고, 아니면 친 그대로 둡니다 (점선)"),
    h(
      "div",
      { class: "range" },
      input,
      out,
      th === undefined
        ? null
        : h(
            "button",
            {
              class: "btn quiet",
              disabled: busy,
              onclick: () => edit({ conversionThreshold: undefined }, "app-" + exe + "-th"),
            },
            "기본으로",
          ),
    ),
    preview,
  );
}

function appCard(exe, rule) {
  const star = exe === "*";
  const edit = (patch, key) => {
    const next = copy(state.apps);
    const merged = { ...(next[exe] || {}), ...patch };
    for (const k of Object.keys(merged)) if (merged[k] === undefined) delete merged[k];
    next[exe] = merged;
    saveApps(next, key);
  };
  const choice = (key, label) =>
    h(
      "div",
      { class: "field" },
      h("span", {}, label),
      dd({
        key: "app-" + exe + "-" + key,
        label: exe + " " + label,
        disabled: busy,
        value: tri(rule[key]),
        options: TRI,
        onchange: (v) => edit({ [key]: triVal(v) }, "app-" + exe + "-" + key),
      }),
    );
  const presets = dd({
    key: "app-" + exe + "-preset",
    label: exe + " 프리셋 적용",
    disabled: busy,
    value: "",
    options: [["", "프리셋 적용…"], ...PRESETS.map((x, i) => [String(i), x[0]])],
    onchange: (v) => {
      if (v !== "") edit(PRESETS[v][1], "app-" + exe + "-th");
    },
  });
  const remove = star
    ? null
    : h(
        "button",
        {
          class: "btn danger",
          disabled: busy,
          "aria-label": exe + " 삭제",
          onclick: () => {
            const next = copy(state.apps);
            delete next[exe];
            saveApps(next);
          },
        },
        "삭제",
      );
  const sum = summary(rule);
  return h(
    "article",
    { class: "app", "aria-label": star ? "모든 앱" : exe },
    h("h3", {}, star ? "모든 앱 (*)" : exe, h("span", { class: "grow" }), presets, remove),
    h(
      "div",
      { class: "sum" },
      sum.length
        ? sum.map((t) => h("span", {}, t))
        : h(
            "span",
            { class: "none" },
            star ? "특별한 규칙 없음 — 기본 동작" : "모두 ‘기본’ — 모든 앱 규칙을 따릅니다",
          ),
    ),
    h(
      "div",
      { class: "fields" },
      choice("enabled", "사용"),
      choice("escapeToApp", "Esc는 앱에게"),
      choice("useSystemIme", "시스템 IME"),
      choice("fullWidth", "일본어 전각"),
      thresholdField(exe, rule, edit),
    ),
  );
}

async function loadRunning() {
  if (running.loading) return;
  running.loading = true;
  if (state) renderApps();
  try {
    running.list = (await api("/api/processes")).apps;
  } catch (e) {
    running.list = [];
    toast(e.message, true);
  }
  running.loading = false;
  if (state) renderApps();
}

function addApp(name, rule, quiet) {
  const n = name.trim().toLowerCase();
  if (!n) return;
  if (!/^[^\\/:*?"<>|]+$/.test(n)) return void toast("파일 이름만 적어 주세요 (경로 없이)", true);
  if (state.apps[n]) return void (quiet || toast("이미 있는 앱입니다", true));
  saveApps({ ...state.apps, [n]: rule }, "app-" + n + "-th");
}

function procList() {
  const box = h("div", {});
  const q = running.filter.trim().toLowerCase();
  const all = (running.list || []).filter((a) => !state.apps[a] && a.includes(q));
  const pages = Math.max(1, Math.ceil(all.length / PAGE_SIZE));
  running.page = Math.min(running.page, pages - 1);
  const grid = h("div", { class: "procs" });
  if (running.loading && !running.list)
    grid.append(h("div", { class: "skeleton" }), h("div", { class: "skeleton" }));
  else if (!all.length)
    grid.append(
      h(
        "div",
        { class: "emptybox" },
        h("b", {}, q ? "찾는 앱이 없습니다" : "규칙을 추가할 앱이 없습니다"),
        q
          ? "다른 이름으로 찾거나, 아래에서 파일 이름을 직접 적어 주세요."
          : "열려 있는 프로그램이 여기에 나옵니다.",
      ),
    );
  else
    grid.append(
      ...all.slice(running.page * PAGE_SIZE, (running.page + 1) * PAGE_SIZE).map((a) =>
        h(
          "div",
          { class: "proc" },
          h("code", { title: a }, a),
          dd({
            key: "proc-" + a,
            label: a + " 규칙 추가",
            disabled: busy,
            value: "",
            options: [
              ["", "규칙 추가…"],
              [BLANK, "기본 규칙으로"],
              ...PRESETS.map((x, i) => [String(i), x[0]]),
            ],
            onchange: (v) => addApp(a, v === BLANK ? {} : { ...PRESETS[v][1] }),
          }),
        ),
      ),
    );
  box.append(grid);
  if (pages > 1) {
    const go = (d) => {
      running.page += d;
      box.replaceWith(procList());
    };
    box.append(
      h(
        "div",
        { class: "pager" },
        h(
          "button",
          {
            class: "btn quiet",
            disabled: running.page === 0,
            "aria-label": "이전 페이지",
            onclick: () => go(-1),
          },
          "←",
        ),
        h("span", { role: "status" }, running.page + 1 + " / " + pages + " · " + all.length + "개"),
        h(
          "button",
          {
            class: "btn quiet",
            disabled: running.page === pages - 1,
            "aria-label": "다음 페이지",
            onclick: () => go(1),
          },
          "→",
        ),
      ),
    );
  }
  return box;
}

function renderApps() {
  const p = $("p-apps");
  p.replaceChildren();
  p.append(
    h(
      "p",
      { class: "note" },
      "앱 실행 파일 이름(대소문자 무시)마다 규칙을 정합니다. ‘기본’은 ‘모든 앱’ 항목을 따릅니다. 터미널은 변환 기준을 엄격하게, 게임은 끄는 식으로 씁니다.",
    ),
  );
  const exes = [
    "*",
    ...Object.keys(state.apps)
      .filter((k) => k !== "*")
      .sort(),
  ];
  p.append(
    h(
      "div",
      { class: "apps" },
      exes.map((e) => appCard(e, state.apps[e] || {})),
    ),
  );

  p.append(h("h2", {}, "실행 중인 앱"));
  const list = h("div", {});
  const filter = h("input", {
    type: "text",
    "data-f": "app-filter",
    placeholder: "앱 이름으로 찾기",
    "aria-label": "실행 중인 앱 찾기",
    autocomplete: "off",
    value: running.filter,
    oninput: (e) => {
      running.filter = e.target.value;
      running.page = 0;
      list.replaceChildren(procList());
    },
  });
  p.append(
    h(
      "div",
      { class: "bar" },
      filter,
      h(
        "button",
        { class: "btn", disabled: running.loading, onclick: loadRunning },
        running.loading ? "불러오는 중…" : "새로고침",
      ),
    ),
    list,
  );
  list.append(procList());

  p.append(h("h2", {}, "직접 추가"));
  const input = h("input", {
    type: "text",
    placeholder: "예: windowsterminal.exe",
    "aria-label": "앱 실행 파일 이름",
    autocomplete: "off",
  });
  const preset = dd({
    label: "처음 적용할 프리셋",
    value: "",
    options: [["", "프리셋 없음"], ...PRESETS.map((x, i) => [String(i), x[0]])],
  });
  const add = () => addApp(input.value, preset.value === "" ? {} : { ...PRESETS[preset.value][1] });
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") add();
  });
  p.append(
    h(
      "div",
      { class: "card pad" },
      h(
        "p",
        { class: "note", style: "margin:0 0 10px" },
        "목록에 없으면 파일 이름을 적거나, .exe 파일을 이 화면으로 끌어다 놓으세요.",
      ),
      h(
        "div",
        { class: "pair" },
        input,
        preset,
        h("button", { class: "btn primary", disabled: busy, onclick: add }, "추가"),
      ),
    ),
  );
}

// Dropping a program file anywhere on this tab adds a rule for it.
const appsPanel = () => $("p-apps");
const carriesFiles = (e) => e.dataTransfer && [...e.dataTransfer.types].includes("Files");
["dragenter", "dragover"].forEach((type) =>
  document.addEventListener(type, (e) => {
    if (appsPanel().hidden || !carriesFiles(e)) return;
    e.preventDefault();
    appsPanel().classList.add("dropping");
  }),
);
document.addEventListener("dragleave", (e) => {
  if (!e.relatedTarget) appsPanel().classList.remove("dropping");
});
document.addEventListener("drop", (e) => {
  appsPanel().classList.remove("dropping");
  if (appsPanel().hidden || !state || !carriesFiles(e)) return;
  e.preventDefault();
  const names = [...e.dataTransfer.files]
    .map((f) => f.name)
    .filter((n) => n.toLowerCase().endsWith(".exe"));
  if (!names.length) return void toast(".exe 파일을 끌어다 놓아 주세요", true);
  addApp(names[0], {});
});
