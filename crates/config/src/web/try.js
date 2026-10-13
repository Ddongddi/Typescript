// 체험 tab: type on a QWERTY keyboard, see what each word becomes. Click a word to see what
// the other languages would have made of it, and try them.

const tryState = {
  text: "", app: "*", seq: 0, timer: 0, order: "",
  segments: [],   // the last answer of /api/try
  open: -1,       // index of the word whose "why" is showing
  picked: {},     // word index -> language the person chose to look at instead
};
const SAMPLES = [["dkssudgktpdy hello world", "한글 + 영어"], ["rkskek git commit -m", "코드 사이에서"], ["kyouhaiitenkidesune", "일본어", "ja"]];
const PREVIEW_TEXT = "dkssudgktpdy hello rkskek git";   // what an app card's slider reads

// The "convert only when sure" setting of an app rule, falling back to the rule for all apps.
function appThreshold(name) {
  const rule = state.apps[name] || {}, all = state.apps["*"] || {};
  return rule.conversionThreshold !== undefined ? rule.conversionThreshold : all.conversionThreshold;
}

async function readTyping(text, threshold) {
  const body = threshold === undefined ? { text } : { text, threshold };
  return (await api("/api/try", body)).segments;
}

// The reading shown for word `i`: the one the engine chose, or the one picked in the why panel.
function shownWord(i) {
  const word = tryState.segments[i], lang = tryState.picked[i];
  if (!lang) return { lang: word.lang, out: word.out, kept: word.kept };
  return { lang, out: word.alts.find((a) => a.lang === lang).out, kept: false };
}

const resultText = () => tryState.segments.map((s, i) => (s.keys ? shownWord(i).out : s.text)).join("");

function drawUnits() {
  const units = $("try-units"), copyBtn = $("try-copy");
  if (!units) return;
  if (!tryState.text.trim()) {
    units.replaceChildren(h("span", { class: "idle" }, "여기에 읽힌 결과가 나타납니다"));
    copyBtn.disabled = true;
    return;
  }
  copyBtn.disabled = false;
  units.replaceChildren(...tryState.segments.flatMap((s, i) => {
    if (!s.keys) {
      if (s.text.includes("\n")) return [h("div", { class: "br" })];
      return s.text.trim() ? [h("span", { class: "sep" }, s.text.trim())] : [];
    }
    const w = shownWord(i), pct = Math.round(s.confidence * 100);
    return [h("button", {
      type: "button", class: "unit l-" + w.lang + (w.kept ? " kept" : ""), "aria-expanded": i === tryState.open,
      title: w.kept ? "확신이 부족해 그대로 둠" : "눌러서 왜 이렇게 읽었는지 보기",
      onclick: () => { tryState.open = tryState.open === i ? -1 : i; drawUnits(); },
    },
      h("span", { class: "u-out" }, w.out),
      h("span", { class: "u-meta" }, s.keys + "  " + (GLYPH[w.lang] || w.lang) + " " + pct),
      h("span", { class: "u-bar" }, h("i", { style: "width:" + pct + "%" })))];
  }));
  drawWhy();
}

function drawWhy() {
  const box = $("try-why"), i = tryState.open;
  if (i < 0 || !tryState.segments[i]) return box.replaceChildren();
  const s = tryState.segments[i], current = shownWord(i).lang;
  const tag = (alt) => (alt.lang === s.lang && !s.kept ? "고른 읽기 · " + Math.round(s.confidence * 100) + "%"
    : alt.lang === s.runnerUp ? "차선" : "");
  box.replaceChildren(h("div", { class: "why" },
    h("h3", {}, h("span", {}, s.keys), h("span", {}, s.kept ? "확신이 부족해 그대로 둠" : "이렇게 읽었습니다")),
    s.why ? h("p", { class: "reason" }, s.why) : null,
    state.order.filter((id) => s.alts.some((a) => a.lang === id)).map((id) => {
      const alt = s.alts.find((a) => a.lang === id);
      return h("button", {
        type: "button", class: "alt", "aria-pressed": id === current,
        onclick: () => { if (id === s.lang) delete tryState.picked[i]; else tryState.picked[i] = id; drawUnits(); },
      }, h("span", { class: "g g-" + id }, GLYPH[id] || "·"), h("span", { class: "o" }, alt.out), h("span", { class: "t" }, tag(alt)));
    })));
}

async function runTry() {
  if (!$("try-units")) return;
  const mine = ++tryState.seq;
  tryState.open = -1; tryState.picked = {};
  if (!tryState.text.trim()) { tryState.segments = []; return drawUnits(); }
  let segments;
  try { segments = await readTyping(tryState.text, appThreshold(tryState.app)); }
  catch (e) { if (mine === tryState.seq) $("try-units").replaceChildren(h("span", { class: "idle", role: "alert" }, e.message)); return; }
  if (mine !== tryState.seq) return;   // a newer keystroke is already on its way
  tryState.segments = segments;
  drawUnits();
}

const scheduleTry = () => { clearTimeout(tryState.timer); tryState.timer = setTimeout(runTry, 180); };
const setTryText = (text) => { const box = $("try-in"); box.value = text; tryState.text = text; runTry(); box.focus(); };

function buildTry() {
  const area = h("textarea", {
    id: "try-in", class: "try-in", rows: 2, spellcheck: "false", autocomplete: "off", "aria-label": "영문 자판으로 친 글", placeholder: "dkssudgktpdy hello world",
    oninput: (e) => { tryState.text = e.target.value; scheduleTry(); },
    onkeydown: (e) => { if (e.key === "Escape") e.target.blur(); },
  });
  const copyResult = async () => {
    try { await navigator.clipboard.writeText(resultText()); toast("복사했습니다"); }
    catch (e) { toast("복사하지 못했습니다", true); }
  };
  $("p-try").append(h("div", { class: "try" },
    h("label", { class: "cap", for: "try-in" }, "영문 자판으로 그냥 치세요"), area,
    h("div", { class: "try-samples", id: "try-chips" }),
    h("div", { class: "try-rule", "aria-hidden": "true" }, h("span", {}, "읽힌 결과")),
    h("div", { class: "units", id: "try-units", "aria-live": "polite" }),
    h("div", { id: "try-why" }),
    h("div", { class: "try-foot" },
      h("span", { id: "try-app" }), h("span", { class: "grow" }),
      h("button", { class: "btn quiet", id: "try-copy", disabled: true, onclick: copyResult }, "결과 복사")),
    h("p", { class: "note", style: "margin-top:26px" }, "단어마다 읽은 언어와 확신도(%)를 보여 줍니다. 단어를 누르면 다른 언어로는 어떻게 읽히는지 보고 바꿔 볼 수 있습니다. 점선은 확신이 부족해 그대로 둔 단어입니다. 실제 입력에서는 앞 단어와 기록한 단어도 영향을 줍니다.")));
}

function updateTry() {
  const app = $("try-app"), chips = $("try-chips");
  if (!app) return;
  const names = ["*", ...Object.keys(state.apps).filter((k) => k !== "*").sort()];
  if (!names.includes(tryState.app)) tryState.app = "*";
  app.replaceChildren(dd({
    label: "적용할 앱 규칙", value: tryState.app, options: names.map((n) => [n, n === "*" ? "모든 앱 기준" : n + " 기준"]),
    onchange: (v) => { tryState.app = v; scheduleTry(); },
  }));
  chips.replaceChildren(h("span", {}, "예시"), ...SAMPLES.filter((x) => !x[2] || state.order.includes(x[2])).map((x) => h("button", { class: "linkbtn", onclick: () => setTryText(x[0]) }, x[0])));
  const order = state.order.join();
  if (order !== tryState.order) { tryState.order = order; runTry(); }
}

// On this tab, typing anywhere goes into the input (like a terminal), and so does pasting.
const tryActive = () => !$("p-try").hidden && !openDd;
document.addEventListener("keydown", (e) => {
  if (!tryActive() || e.ctrlKey || e.metaKey || e.altKey || editable(e.target)) return;
  const typing = e.key.length === 1 || e.key === "Backspace";
  if (!typing || (e.target.tagName === "BUTTON" && e.key === " ")) return;   // a focused button keeps Space
  $("try-in").focus();
});
document.addEventListener("paste", (e) => {
  if (!tryActive() || editable(e.target)) return;
  const text = e.clipboardData && e.clipboardData.getData("text");
  if (!text) return;
  e.preventDefault();
  const area = $("try-in");
  area.focus();
  area.setRangeText(text, area.selectionStart, area.selectionEnd, "end");
  area.dispatchEvent(new Event("input"));
});
