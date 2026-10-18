"use strict";
// The first-run page: a title and one thing under it, and a segmented bar on top.
//   1. "안녕하세요", then type "안녕 Tacit" the usual way (the Han/Eng key flashes red);
//   2. "is the Han/Eng key really needed?" -> try dkssud Tacit without it;
//   3. "your Han/Eng key becomes a shortcut" (a slot machine of shortcuts);
//   4. choose the languages, switch to tacit, practice (type freely; only the result is checked).

const PASS_PAUSE = 600; // ms the check mark stays before the next screen
const ASK_PAUSE = 1800; // ms "자, 어떠신가요?" stays: time to look at what the box made
const TITLE_IN = 450; // ms a new title takes to come in, before what depends on it wakes
const INTRO_TEXT = "안녕 Tacit";
const TRIAL_KEYS = "dkssud Tacit";
// What the box shows after each of the first six keys of the trial: the Korean the program makes
// of "dkssud" as it goes (the languages are not installed yet at this point, so it is scripted).
const TRIAL_HANGUL = ["ㅇ", "아", "안", "안ㄴ", "안녀", "안녕"];

// What each practice screen asks for: the keys, and the text they should make.
const hl = (text) => h("span", { class: "hl" }, text); // the text to type, in the accent colour
const PRACTICE = {
  ko: { title: () => [hl("dkssud"), " 를 쳐 보세요"], keys: "dkssud", expect: "안녕" },
  mixed: {
    title: () => [hl("hello dkssud"), " 를 쳐 보세요"],
    keys: "hello dkssud",
    expect: "hello 안녕",
  },
  ja: { title: () => [hl("nihongo"), " 를 쳐 보세요"], keys: "nihongo", expect: "にほんご" },
};
const SWITCH_KEYS = {
  windows: ["Win", "Space"],
  macos: ["Control", "Space"],
  linux: ["Super", "Space"],
};
// Shortcuts the Han/Eng key can send, spun through the slot machine.
const COMBOS = {
  windows: [
    ["Ctrl", "Z"],
    ["Ctrl", "C"],
    ["Ctrl", "V"],
    ["Win", "D"],
    ["Win", "L"],
    ["Alt", "Tab"],
  ],
  macos: [
    ["⌘", "Z"],
    ["⌘", "C"],
    ["⌘", "V"],
    ["⌘", "Tab"],
    ["⌥", "Tab"],
  ],
  linux: [
    ["Ctrl", "Z"],
    ["Ctrl", "C"],
    ["Ctrl", "V"],
    ["Super", "D"],
    ["Alt", "Tab"],
  ],
};
const SLOT_EVERY = 1600; // ms between flips of the slot machine: steady and slow
const LIST_ORDER = ["en", "ko", "ja"]; // the order they are offered in, and the Esc order they get
const DEFAULT_ON = ["en", "ko"];
const INTRO_SEGMENTS = 5; // hello/type, "needed?", trial, shortcut, languages

// The key that types `ch` on a QWERTY keyboard: its `code` and whether Shift is held.
function keyOf(ch) {
  if (ch === " ") return { code: "Space", shift: false };
  if (/[a-z]/.test(ch)) return { code: "Key" + ch.toUpperCase(), shift: false };
  if (/[A-Z]/.test(ch)) return { code: "Key" + ch, shift: true };
  if (/[0-9]/.test(ch)) return { code: "Digit" + ch, shift: false };
  return null;
}

// Is this key press the one `keys[at]` asks for? (`code`, not `key`: with an input method on,
// `key` is "Process".)
function isNext(keys, at, e) {
  const want = keyOf(keys[at] || "");
  return !!want && e.code === want.code && e.shiftKey === want.shift;
}

// The Han/Eng key of a Korean keyboard.
const isHangulKey = (e) => e.code === "Lang1" || e.key === "HangulMode";

// What the trial box shows after `at` keys of TRIAL_KEYS.
function trialText(at) {
  if (at <= 0) return "";
  if (at <= TRIAL_HANGUL.length) return TRIAL_HANGUL[at - 1];
  return "안녕" + TRIAL_KEYS.slice(TRIAL_HANGUL.length, at);
}

if (typeof module !== "undefined") module.exports = { keyOf, isNext, trialText };

const view =
  typeof document === "undefined"
    ? null
    : {
        title: $("title"),
        stage: $("stage"),
        later: $("later"),
        bar: $("bar"),
      };
let imeKnown = false; // whether the system answers which input method is selected
let imeIsOurs = false;
let beatEvery = 5000;
let epoch = 0; // counts screens: a timer of an earlier screen does nothing
let skipAction = () => leave();
const progress = { total: INTRO_SEGMENTS + 4, cursor: 0 }; // 4: switch, then three Korean screens (English + Korean)

function leave() {
  location.href = "/";
}

// The page must keep pinging, or the program ends after 3 minutes of silence.
async function beat() {
  try {
    const answer = await api("/api/heartbeat", {});
    imeKnown = !!(answer && answer.ime);
    imeIsOurs = imeKnown && answer.ime.tacit;
  } catch (e) {
    /* the next ping tries again */
  }
  setTimeout(beat, beatEvery);
}

// ---- motion and the bar -------------------------------------------------------------------

// Plays a one-shot animation class again, even when it was played a moment ago.
function replay(el, cls) {
  el.classList.remove(cls);
  void el.offsetWidth;
  el.classList.add(cls);
}

// Same, and takes the class off when it ends (a red border that comes and goes).
function flash(el, cls) {
  replay(el, cls);
  el.addEventListener("animationend", () => el.classList.remove(cls), { once: true });
}

function drawBar() {
  const bar = view.bar;
  if (bar.childElementCount !== progress.total)
    bar.replaceChildren(...Array.from({ length: progress.total }, () => h("i", {})));
  [...bar.children].forEach((seg, i) => {
    seg.className = i <= progress.cursor ? "on" : ""; // a segment is lit or not, never half
  });
  bar.setAttribute("aria-valuemax", String(progress.total));
  bar.setAttribute("aria-valuenow", String(progress.cursor));
}

function goto(segment) {
  progress.cursor = segment;
  drawBar();
}

const setTitle = (title) => view.title.replaceChildren(...[].concat(title));

// A new screen: the title slides in, then each piece of the stage one after the other.
function show(title, ...stage) {
  epoch += 1;
  view.title.classList.remove("good", "fly", "one");
  view.stage.classList.remove("busy");
  setTitle(title);
  replay(view.title, "swap");
  stage.forEach((el, i) => el.style.setProperty("--i", i));
  view.stage.replaceChildren(...stage);
}

// The title flies up and a new one comes in, the rest of the screen stays. `then` runs once the
// new title is in place.
function retitle(title, then) {
  const mine = epoch;
  view.title.classList.add("fly");
  setTimeout(() => {
    if (mine !== epoch) return;
    view.title.classList.remove("fly");
    setTitle(title);
    replay(view.title, "swap");
    if (then)
      setTimeout(() => {
        if (mine === epoch) then();
      }, TITLE_IN);
  }, 320);
}

// Shows the check mark (or `message`), then the next screen after `pause` ms.
function pass(box, next, message, pause = PASS_PAUSE) {
  if (box) replay(box, "done");
  view.title.textContent = message || "✓";
  view.title.classList.remove("swap");
  if (message) view.title.classList.add("one"); // a sentence: on one line, never broken in two
  replay(view.title, message ? "swap" : "good");
  setTimeout(next, pause);
}

// ---- the opening ---------------------------------------------------------------------------

// Needs nothing from the server, so it is drawn at once; the state loads meanwhile.
function intro() {
  skipAction = chooseLanguages;
  view.later.hidden = false;
  goto(0);
  askIntro();
}

// "안녕하세요", and under it in grey what to ask of you: type "안녕 Tacit" with your own
// input method. Every press of the Han/Eng key shows what this page is about.
function askIntro() {
  const hint = h("p", { class: "hint", role: "status" });
  const aside = h("p", { class: "aside" }, hl(INTRO_TEXT), "를 입력해 보세요");
  const input = h("input", {
    type: "text",
    class: "practice",
    placeholder: INTRO_TEXT,
    "aria-label": INTRO_TEXT,
    autocomplete: "off",
    autocapitalize: "off",
    spellcheck: "false",
  });
  const next = h("button", { type: "button", class: "btn primary wide", disabled: true }, "계속");
  show("안녕하세요", aside, input, next, hint);
  input.focus();
  let finished = false;
  input.addEventListener("keydown", (e) => {
    if (isHangulKey(e)) flash(input, "nope");
  });
  const check = () => {
    const text = input.value.normalize("NFC");
    hint.textContent =
      !finished && text.length >= 6 && /^[\x20-\x7e]*$/.test(text)
        ? "한/영키를 눌러 한글로 바꿔 쳐 보세요"
        : "";
    if (finished || text !== INTRO_TEXT) return;
    finished = true;
    input.readOnly = true;
    input.classList.add("done");
    goto(1);
    aside.hidden = true; // the request is done
    // First the question, then the button wakes up with its answer.
    retitle([hanKey(), "키가 꼭 필요한 키일까요?"], () => {
      next.textContent = "아니요";
      next.disabled = false;
      replay(next, "lit");
      next.focus();
    });
  };
  input.addEventListener("input", check);
  input.addEventListener("compositionend", check);
  next.addEventListener("click", tryIt);
}

// The same sentence without the key: the box takes the letters and shows what the program makes.
function tryIt() {
  goto(2);
  const field = h("div", {
    class: "practice trial empty",
    role: "textbox",
    tabindex: "0",
    "aria-label": TRIAL_KEYS,
    "data-placeholder": TRIAL_KEYS,
  });
  const hint = h("p", { class: "hint", role: "status" });
  let at = 0;
  const draw = () => {
    const text = trialText(at);
    const composing = at >= 1 && at <= TRIAL_HANGUL.length; // the last syllable is still being made
    field.classList.toggle("empty", !text);
    field.replaceChildren(
      composing ? text.slice(0, -1) : text,
      composing ? h("u", {}, text.slice(-1)) : "",
      h("i", { class: "caret" }),
    );
  };
  field.addEventListener("keydown", (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey || e.key === "Tab" || e.key === "Shift") return;
    e.preventDefault();
    if (isHangulKey(e)) {
      flash(field, "nope");
      hint.textContent = "한/영키 없이 쳐 보세요";
      return;
    }
    if (!isNext(TRIAL_KEYS, at, e)) return; // the demo only plays the sentence: other keys are ignored, quietly
    hint.textContent = "";
    at += 1;
    draw();
    if (at === TRIAL_KEYS.length) pass(field, shortcutScreen, "자, 어떠신가요?", ASK_PAUSE);
  });
  show([hanKey(), "키를 누르지 않고 ", hl(TRIAL_KEYS), " 를 쳐 보세요"], field, hint);
  draw();
  field.focus();
}

// The Han/Eng key as a keycap, for titles.
const hanKey = () => h("kbd", {}, "한/영");

// A slot machine of keycaps: every SLOT_EVERY ms the next shortcut rolls in.
function slotMachine(os) {
  const combos = COMBOS[os] || COMBOS.linux;
  const slot = h("span", { class: "slot", role: "img", "aria-label": "단축키" });
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  let n = -1;
  const flip = () => {
    if (n >= 0 && !slot.isConnected) {
      clearInterval(timer);
      return;
    } // the screen is gone
    n = (n + 1) % combos.length;
    const old = slot.lastElementChild;
    slot.append(h("span", { class: "reel in" }, ...combos[n].map((k) => h("kbd", {}, k))));
    if (!old) return;
    if (still) {
      old.remove();
      return;
    } // no motion: the old one must not linger under the new
    old.classList.add("out");
    old.addEventListener("animationend", () => old.remove(), { once: true });
  };
  const timer = setInterval(flip, SLOT_EVERY);
  flip();
  return slot;
}

function shortcutScreen() {
  goto(3);
  const go = h("button", { type: "button", class: "btn primary wide" }, "해볼까요?");
  go.addEventListener("click", chooseLanguages);
  const aside = h("p", { class: "aside" }, "더 이상 필요없을 키거든요 :)");
  show([hanKey(), "키를, ", slotMachine((appState || {}).os), "로 바꿔드립니다"], aside, go);
  view.title.classList.add("one"); // this title stays on one line
  go.focus();
}

// ---- the languages -------------------------------------------------------------------------

const hasPack = (state) =>
  state.order.some((id) => (state.languages.find((l) => l.id === id) || {}).installed);

// The list of languages (English and Korean ticked), then [시작], which installs what is ticked
// and goes on to the practice. No way past it without a language, so no skip button here.
async function chooseLanguages() {
  const state = appState || (await loadState().catch(() => null));
  if (!state) return; // the error screen is up already
  if (hasPack(state)) {
    steps(state);
    return;
  } // seen again later: the languages are there already
  goto(4);
  view.later.hidden = true;
  const langs = LIST_ORDER.map((id) => state.languages.find((l) => l.id === id)).filter(Boolean);
  const picked = new Set(langs.map((l) => l.id).filter((id) => DEFAULT_ON.includes(id)));
  const hint = h("p", { class: "hint", role: "status" });
  const start = h("button", { type: "button", class: "btn primary wide" }, "시작");
  const sync = () => {
    start.disabled = !picked.size;
  };
  const rows = langs.map((l, i) => {
    const box = h("input", { type: "checkbox", checked: picked.has(l.id) });
    box.addEventListener("change", () => {
      if (box.checked) picked.add(l.id);
      else picked.delete(l.id);
      sync();
    });
    return h(
      "li",
      { style: "--i:" + i, "data-lang": l.id },
      h(
        "label",
        { class: "lang" },
        box,
        h("span", { class: "tick", "aria-hidden": "true" }),
        h("span", { class: "name" }, l.name),
        h("small", {}, l.installed ? "받아 둠" : mb(l.size)),
      ),
    );
  });
  start.addEventListener("click", () =>
    install(
      LIST_ORDER.filter((id) => picked.has(id)),
      langs,
      [start],
      hint,
      steps,
    ),
  );
  show(
    "Tacit",
    h("p", { class: "caption" }, "쓸 언어를 고르세요"),
    h("ul", { class: "langs" }, ...rows),
    start,
    hint,
  );
  sync();
  start.focus();
}

// Gets the languages one after the other and turns them on in `ids` order; then `after` (steps
// gets the new state). The server answers once a language is done, so progress is per language:
// each row says where it is, and the button fills by the share of the download that is finished.
async function install(ids, langs, buttons, hint, after) {
  const [start] = buttons;
  const weight = (id) => Math.max((langs.find((l) => l.id === id) || {}).size || 0, 1);
  const total = ids.reduce((sum, id) => sum + weight(id), 0);
  const row = (id) => view.stage.querySelector(`[data-lang="${id}"] small`);
  const say = (id, text, cls) => {
    const el = row(id);
    if (el) {
      el.textContent = text;
      el.className = cls || "";
    }
  };
  const fill = (bytes) => start.style.setProperty("--p", Math.round((100 * bytes) / total) + "%");
  let finished = 0,
    current = null;
  buttons.forEach((b) => {
    b.disabled = true;
  });
  view.stage.classList.add("busy");
  hint.textContent = "";
  start.classList.add("loading");
  fill(0);
  try {
    for (const [i, id] of ids.entries()) {
      current = id;
      say(id, "받는 중…", "working");
      start.textContent = `받는 중… ${i + 1}/${ids.length}`;
      await api("/api/languages", { action: "add", id });
      finished += weight(id);
      fill(finished);
      say(id, "완료", "ok");
    }
    current = null;
    start.textContent = "켜는 중…";
    const state = await api("/api/languages", { action: "order", ids });
    start.textContent = "준비됐어요";
    setTimeout(() => after(state), 450); // a moment to see the bar full
  } catch (e) {
    if (current) say(current, "실패", "bad");
    hint.textContent = e.message;
    start.textContent = "다시 시도";
    start.classList.remove("loading");
    view.stage.classList.remove("busy");
    buttons.forEach((b) => {
      b.disabled = false;
    });
  }
}

// ---- the practice --------------------------------------------------------------------------

function switchScreen(state, next) {
  const keys = SWITCH_KEYS[state.os] || ["Super", "Space"];
  const cap = h(
    "div",
    { class: "cap", "aria-label": keys.join(" + ") },
    ...keys.flatMap((k, i) => (i ? ["+", h("kbd", {}, k)] : [h("kbd", {}, k)])),
  );
  const unsure = h("button", { type: "button", class: "btn", onclick: next }, "바꿨어요");
  unsure.hidden = true;
  show("입력기를 Tacit으로 바꾸세요", cap, unsure);
  beatEvery = 1000;
  const wait = setInterval(() => {
    if (imeKnown && imeIsOurs) {
      clearInterval(wait);
      beatEvery = 5000;
      pass(null, next);
    } else if (!imeKnown) unsure.hidden = false; // this system does not say: the user does
  }, 500);
  unsure.addEventListener("click", () => {
    clearInterval(wait);
    beatEvery = 5000;
  });
}

// One text to type. `spec.expect` null: after the keys, Esc has to change the text.
function typingScreen(spec, next) {
  const hint = h("p", { class: "hint", role: "status" });
  const input = h("input", {
    type: "text",
    class: "practice",
    placeholder: spec.keys,
    "aria-label": spec.keys,
    autocomplete: "off",
    autocapitalize: "off",
    spellcheck: "false",
  });
  let composing = false,
    finished = false,
    timer = 0;
  const done = () => {
    if (!finished) {
      finished = true;
      pass(input, next);
    }
  };
  // Free typing: nothing is held back, only the result is checked.
  const check = () => {
    clearTimeout(timer);
    hint.textContent = "";
    if (input.value === spec.expect) {
      done();
      return;
    }
    if (composing || input.value.length < spec.expect.length) return; // still typing
    // Typed enough and it is not the text: say why, after a moment (the last key may still be coming).
    timer = setTimeout(() => {
      if (finished || input.value === spec.expect) return;
      hint.textContent =
        imeKnown && !imeIsOurs
          ? "Tacit이 선택되어 있지 않아요. 지우고 다시 쳐 보세요"
          : "다른 글자가 나왔어요. 지우고 다시 쳐 보세요";
    }, 800);
  };
  input.addEventListener("compositionstart", () => {
    composing = true;
  });
  input.addEventListener("compositionend", () => {
    composing = false;
    check();
  });
  input.addEventListener("input", check);
  show(spec.title(), input, hint);
  input.focus();
}

// The word to press Esc on: 깨짐 typed on English keys is "Rowla". The engine may read it as
// English or as the Korean, and that changes as it improves, so the screen works either way:
// Esc turns what is shown into the other reading, and a second one brings it back.
const ESC_WORD = { text: "깨짐", keys: "Rowla" };
const ESC_TAUGHT = "Esc로 2번 고치신 단어는 기억해 드립니다";
const TAUGHT_PAUSE = 2800; // ms the sentence above stays: it is the point of the screen

function escScreen(next) {
  const hint = h("p", { class: "hint", role: "status" });
  const input = h("input", {
    type: "text",
    class: "practice",
    placeholder: ESC_WORD.keys,
    "aria-label": ESC_WORD.keys,
    autocomplete: "off",
    autocapitalize: "off",
    spellcheck: "false",
  });
  const say = (text) => {
    hint.textContent = text;
    replay(hint, "slide");
  };
  // type -> esc (it shows as typed and Esc has to change it) or toggle (it came out right:
  // Esc shows the other reading, which sends it to esc to be brought back) -> confirm (Space)
  let phase = "type";
  let composing = false,
    finished = false;
  const check = () => {
    if (finished) return;
    const value = input.value;
    if (phase === "type") {
      if (value !== ESC_WORD.keys && value !== ESC_WORD.text) return;
      const wrong = value === ESC_WORD.keys; // the program kept it as English
      phase = wrong ? "esc" : "toggle";
      retitle([hl("Esc"), "를 눌러 보세요"]);
      say(
        wrong
          ? "판별에 실패한 글씨는 Esc로 바꿀 수 있어요"
          : "맞게 나와도 Esc는 다른 읽기로 바꿔 줘요",
      );
    } else if (phase === "toggle") {
      if (value !== ESC_WORD.text && value !== "") {
        phase = "esc";
        say("한 번 더 누르면 돌아와요");
      }
    } else if (phase === "esc") {
      if (value === ESC_WORD.text) {
        phase = "confirm";
        say("Space를 눌러 확정하세요");
      }
    } else if (!composing && value.trim() === ESC_WORD.text) {
      finished = true;
      pass(input, next, ESC_TAUGHT, TAUGHT_PAUSE);
    }
  };
  input.addEventListener("compositionstart", () => {
    composing = true;
  });
  input.addEventListener("compositionend", () => {
    composing = false;
    check();
  });
  input.addEventListener("input", check);
  input.addEventListener("keyup", () => setTimeout(check, 50)); // Esc may change it without an input event
  show([hl(ESC_WORD.text), " 을 쳐 보세요"], input, hint);
  input.focus();
}

function finish() {
  goto(progress.total);
  show(
    "설명은 여기까지에요!",
    h("p", { class: "aside" }, "앞으로의 업데이트도 잘 부탁드려요 :)"),
    h("button", { type: "button", class: "btn primary wide", onclick: leave }, "설정 열기"),
  );
  view.later.hidden = true;
}

// The screens for the languages that are on.
function steps(state) {
  const has = (id) => state.order.includes(id);
  const screens = [(n) => switchScreen(state, n)];
  if (has("ko")) screens.push((n) => typingScreen(PRACTICE.ko, n));
  if (has("ko") && has("en")) screens.push((n) => typingScreen(PRACTICE.mixed, n));
  if (has("ko")) screens.push(escScreen);
  if (has("ja")) screens.push((n) => typingScreen(PRACTICE.ja, n));
  progress.total = INTRO_SEGMENTS + screens.length;
  skipAction = leave;
  view.later.hidden = false;
  const go = (i) => {
    if (i >= screens.length) {
      finish();
      return;
    }
    goto(INTRO_SEGMENTS + i);
    screens[i](() => go(i + 1));
  };
  go(0);
}

let appState = null; // GET /api/state, once it has come
const loadState = () => api("/api/state").then((state) => (appState = state));

if (view) {
  view.later.addEventListener("click", () => skipAction());
  drawBar();
  beat();
  intro();
  loadState().catch((e) => {
    show(
      "연결할 수 없어요",
      h(
        "p",
        { class: "hint" },
        e.message + " 터미널에서 tacit-config welcome 을 다시 실행해 주세요.",
      ),
    );
  });
}
