// A chat to try the input in, on every tab: the keys go through the real input method (the server
// plays them with the saved settings), and what it does not take is what the chat gets. With
// "Enter confirms only" on (입력 설정), a message takes two Enters; off, one. Each message says how many.

const chat = {
  messages: [],            // {text, enters}
  keys: [],                // the draft, as the keys pressed
  draft: { text: "", marked: null },
  hint: "",
  queue: Promise.resolve(), // keys are played one after the other
};

const PREVIEW_TEXT = "dkssudgktpdy hello rkskek git";   // what an app card's slider reads

// The "convert only when sure" setting of an app rule, falling back to the rule for all apps.
function appThreshold(name) {
  const rule = state.apps[name] || {}, all = state.apps["*"] || {};
  return rule.conversionThreshold !== undefined ? rule.conversionThreshold : all.conversionThreshold;
}

// What the engine makes of typed letters, word by word (/api/try).
async function readTyping(text, threshold) {
  const body = threshold === undefined ? { text } : { text, threshold };
  return (await api("/api/try", body)).segments;
}

// The letters a draft was typed with (Backspace takes one back; Enter and Tab are not letters).
function typedText(keys) {
  const typed = [];
  for (const k of keys) {
    if (k.length === 1) typed.push(k);
    else if (k === "Backspace") typed.pop();
  }
  return typed.join("");
}

// The key as the server knows it, or null for keys the chat leaves alone.
function chatKey(e) {
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  if (e.key === "Escape") return e.shiftKey ? "Cycle" : "Escape";
  if (["Enter", "Tab", "Backspace"].includes(e.key)) return e.key;
  return e.key.length === 1 ? e.key : null;
}

async function playKey(key) {
  const keys = [...chat.keys, key];
  let played;
  try { played = await api("/api/play", { keys }); }
  catch (e) { return toast(e.message, true); }
  const enters = keys.filter((k) => k === "Enter").length;
  if (played.submitted) {
    if (played.text.trim()) chat.messages.push({ text: played.text, enters, typed: typedText(keys) });
    chat.keys = [];
    chat.draft = { text: "", marked: null };
    chat.hint = "";
  } else {
    chat.keys = keys;
    chat.draft = played;
    chat.hint = key === "Enter" && played.text ? "확정했습니다. 아직 전송되지 않았습니다 — 한 번 더 Enter" : "";
  }
  drawChat();
}

// Redraws the draft on every key; the log only when a message came or went (rebuilding the
// bubbles on each key made them flash as their entrance animation started over).
function drawChat() {
  const log = $("chat-log"), input = $("chat-in");
  if (!log) return;
  if (log.dataset.count !== String(chat.messages.length)) {
    log.dataset.count = String(chat.messages.length);
    log.replaceChildren(...chat.messages.map(bubble));
    if (!chat.messages.length) log.append(h("p", { class: "idle" }, h("b", {}, "dkssud"), " 를 치고 Enter"));
    log.scrollTop = log.scrollHeight;
    showWhy(null, null);
  }
  const { text, marked } = chat.draft;
  const [at, len] = marked || [text.length, 0];
  const shown = [text.slice(0, at), len ? h("u", {}, text.slice(at, at + len)) : "", h("i", { class: "caret" })];
  input.replaceChildren(...shown);   // (never null: it would be written as the word "null")
  input.classList.toggle("empty", !text);
  $("chat-hint").textContent = chat.hint;
}

// A message: the words it was read as are underlined on hover, as long as the confidence.
function bubble(m) {
  const said = h("span", { class: "said" }, m.text);
  const reveal = async () => {
    if (!m.reading) m.reading = readTyping(m.typed, appThreshold("*")).catch(() => []);
    const segments = await m.reading;
    m.segments = segments;
    const text = segments.map((s) => (s.keys ? s.out : s.text)).join("");
    if (!segments.length || text !== m.text || said.classList.contains("read")) return;   // not what was typed (Tab, ...)
    said.classList.add("read");
    said.replaceChildren(...segments.map((s, i) => (s.keys
      ? h("span", { class: "w l-" + s.lang + (s.kept ? " kept" : ""), style: "--p:" + Math.round(s.confidence * 100) + "%", "data-i": i }, s.out)
      : s.text)));
  };
  reveal();   // read ahead, so the words are already marked up when the pointer arrives
  const hover = (e) => {
    const w = e.target.closest && e.target.closest(".w");
    showWhy(m, w);
  };
  return h("div", { class: "bubble", tabindex: "0", onmouseover: hover, onmouseleave: () => showWhy(null, null), onblur: () => showWhy(null, null) },
    said, h("span", { class: "sent" + (m.enters > 1 ? " two" : "") }, "Enter ×" + m.enters));
}

// The line under the log: what the word under the pointer was read as.
function showWhy(m, el) {
  const box = $("chat-why");
  if (!box) return;
  const s = m && el && m.segments ? m.segments[Number(el.dataset.i)] : null;
  if (!s) {
    box.replaceChildren(chat.messages.length ? "메시지에 올리면 단어마다 읽은 결과를 밑줄로 보여 줍니다" : "");
    return;
  }
  const second = s.alts.find((a) => a.lang === s.runnerUp);
  box.replaceChildren(h("b", {}, s.keys), " → ", h("b", { class: "o" }, s.out), "  ",
    s.kept ? "확신이 부족해 그대로 둠" : langName(s.lang) + " " + Math.round(s.confidence * 100) + "%",
    second ? "  ·  차선 " + langName(s.runnerUp) + " " + second.out : "");
}

// A letter typed on an empty part of the page (not in a field, not on a control) goes to the chat.
function typeIntoChat(e) {
  const key = chatKey(e);
  const t = e.target;
  if (!key || key.length !== 1 || !$("chat-in")) return false;
  if (t && t !== document.body && t !== document.documentElement && t.closest &&
      t.closest("button, a, input, select, textarea, summary, [role=tab], [role=textbox], [role=option], [role=listbox]")) return false;
  e.preventDefault();
  $("chat-in").focus();
  chat.queue = chat.queue.then(() => playKey(key));
  return true;
}

function clearChat() {
  chat.messages = []; chat.keys = []; chat.draft = { text: "", marked: null }; chat.hint = "";
  drawChat();
  const box = $("chat-in");
  if (box) box.focus();
}

function chatBox() {
  const send = (key) => { chat.queue = chat.queue.then(() => playKey(key)); const box = $("chat-in"); if (box) box.focus(); };
  const box = h("section", { class: "chat", "aria-label": "채팅으로 체험" },
    h("div", { class: "chat-head" },
      h("b", {}, "체험"), h("span", {}),
      h("button", { type: "button", class: "btn quiet", "data-f": "chat-clear", onclick: clearChat }, "지우기")),
    h("div", { id: "chat-log", class: "chat-log", role: "log", "aria-live": "polite" }),
    h("p", { id: "chat-why", class: "chat-why", "aria-live": "polite" }),
    h("div", { class: "chat-row" },
      h("div", {
        id: "chat-in", class: "chat-in", role: "textbox", tabindex: "0", "data-f": "chat-in", "aria-label": "메시지 입력 (영문 자판)",
        onkeydown: (e) => {
          const key = chatKey(e);
          if (!key) return;
          e.preventDefault();
          chat.queue = chat.queue.then(() => playKey(key));
        },
      }),
      h("button", { type: "button", class: "chat-send", "aria-label": "Enter", onclick: () => send("Enter") }, "↵")),
    h("p", { id: "chat-hint", class: "chat-hint", "aria-live": "polite" }));
  queueMicrotask(drawChat);
  return box;
}

// The pane on the right, shown on every tab: drawn again with the page, the draft is kept.
function renderChat() {
  $("chat-pane").replaceChildren(chatBox());
}
