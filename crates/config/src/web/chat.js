// A chat to try the input settings in: the keys go through the real input method (the server
// plays them with the saved settings), and what it does not take is what the chat gets. With
// "Enter confirms only" on, a message takes two Enters; off, one. Each message says how many.

const chat = {
  messages: [],            // {text, enters}
  keys: [],                // the draft, as the keys pressed
  draft: { text: "", marked: null },
  hint: "",
  queue: Promise.resolve(), // keys are played one after the other
};

const ENTER_MODES = [
  [false, "Enter 1번 → 바로 전송"],
  [true, "Enter 2번 (확정 → 전송)"],
];

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
    if (played.text.trim()) chat.messages.push({ text: played.text, enters });
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

function drawChat() {
  const log = $("chat-log"), input = $("chat-in");
  if (!log) return;
  log.replaceChildren(...chat.messages.map((m) => h("div", { class: "bubble" },
    h("span", { class: "said" }, m.text),
    h("span", { class: "sent" + (m.enters > 1 ? " two" : "") }, "Enter ×" + m.enters))));
  if (!chat.messages.length) log.append(h("p", { class: "idle" }, "메시지를 보내 보세요. dkssud 를 치고 Enter."));
  log.scrollTop = log.scrollHeight;
  const { text, marked } = chat.draft;
  const [at, len] = marked || [text.length, 0];
  const shown = [text.slice(0, at), len ? h("u", {}, text.slice(at, at + len)) : "", h("i", { class: "caret" })];
  input.replaceChildren(...shown);   // (never null: it would be written as the word "null")
  input.classList.toggle("empty", !text);
  $("chat-hint").textContent = chat.hint;
}

function enterMode() {
  const on = state.settings.returnConfirmsOnly;
  const change = (value) => {
    chat.keys = []; chat.draft = { text: "", marked: null }; chat.hint = "";
    act("저장했습니다", "/api/settings", { returnConfirmsOnly: value }, "chat-mode-" + value,
      { path: "/api/settings", body: { returnConfirmsOnly: on } });
  };
  return h("div", { class: "seg", role: "group", "aria-label": "Enter 동작" },
    ENTER_MODES.map(([value, label]) => h("button", {
      type: "button", class: "btn", "data-f": "chat-mode-" + value, "aria-pressed": on === value, disabled: busy,
      onclick: () => on !== value && change(value),
    }, label)));
}

function chatBox() {
  const box = h("section", { class: "chat", "aria-label": "채팅으로 체험" },
    h("div", { class: "chat-head" },
      h("span", {}, "지금 설정으로 채팅해 보세요. 입력은 실제 입력기와 같은 방식으로 처리됩니다."), enterMode()),
    h("div", { id: "chat-log", class: "chat-log", role: "log", "aria-live": "polite" }),
    h("div", {
      id: "chat-in", class: "chat-in", role: "textbox", tabindex: "0", "data-f": "chat-in", "aria-label": "메시지 입력 (영문 자판)",
      onkeydown: (e) => {
        const key = chatKey(e);
        if (!key) return;
        e.preventDefault();
        chat.queue = chat.queue.then(() => playKey(key));
      },
    }),
    h("p", { id: "chat-hint", class: "chat-hint", "aria-live": "polite" }));
  queueMicrotask(drawChat);
  return box;
}
