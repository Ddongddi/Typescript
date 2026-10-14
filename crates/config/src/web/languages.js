// 언어 tab: the Esc order (the arrows on each row), and one row per language.

const LANGUAGES = "/api/languages";
const ESC_WORD = "dkssud";
const esc = { key: "", reads: {}, at: 0 };   // what each language makes of ESC_WORD; which one shows

const setOrder = (ids, label, key) =>
  act(label, LANGUAGES, { action: "order", ids }, key, { path: LANGUAGES, body: { action: "order", ids: state.order.slice() } });

function move(id, delta) {
  const order = state.order.slice();
  const i = order.indexOf(id), j = i + delta;
  if (j < 0 || j >= order.length) return;
  [order[i], order[j]] = [order[j], order[i]];
  setOrder(order, "순서를 바꿨습니다", "mv-" + id + (delta < 0 ? "-up" : "-down"));
}

// A key to press: each press shows the next language's reading of the same word, as Esc does.
function pressEsc() {
  if (!state || !state.order.length) return;
  esc.at = (esc.at + 1) % state.order.length;
  drawEsc();
}

function drawEsc() {
  const shown = $("esc-shown");
  if (!shown) return;
  const id = state.order[esc.at % state.order.length];
  shown.className = "shown g-" + id;
  shown.textContent = esc.reads[id] || "…";
  shown.style.animation = "none";
  void shown.offsetWidth;   // restart the little rise animation
  shown.style.animation = "";
}

async function loadEsc() {
  const key = state.order.join();
  if (esc.key === key) return drawEsc();
  esc.key = key;
  try {
    const word = (await api("/api/try", { text: ESC_WORD })).segments[0];
    esc.reads = Object.fromEntries(word.alts.map((a) => [a.lang, a.out]));
    esc.at = Math.max(0, state.order.indexOf(word.lang));
  } catch (e) { esc.reads = {}; }
  drawEsc();
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && !e.repeat && !$("p-lang").hidden && !openDd && !editable(e.target)) pressEsc();
});

function langActions(l, position) {
  const on = position != null, working = pending === "lang-" + l.id;
  const toggle = (label, body, text, cls) => h("button", {
    class: "btn " + cls, "data-f": "lang-" + l.id, disabled: busy, onclick: () => act(label, LANGUAGES, body, "lang-" + l.id, undoOf(body)),
  }, text);
  if (on) {
    return [
      h("span", { class: "seg", role: "group", "aria-label": l.name + " 순서" },
        h("button", { class: "btn", "data-f": "mv-" + l.id + "-up", disabled: busy || position === 0, "aria-label": l.name + " 위로", onclick: () => move(l.id, -1) }, "↑"),
        h("button", { class: "btn", "data-f": "mv-" + l.id + "-down", disabled: busy || position === state.order.length - 1, "aria-label": l.name + " 아래로", onclick: () => move(l.id, 1) }, "↓")),
      h("button", {
        class: "btn danger", "data-f": "lang-" + l.id, disabled: busy || state.order.length < 2, title: state.order.length < 2 ? "하나는 켜 두어야 합니다" : "",
        onclick: () => act(l.name + "을(를) 껐습니다", LANGUAGES, { action: "remove", id: l.id }, "lang-" + l.id, undoOf({ action: "remove", id: l.id })),
      }, "끄기"),
    ];
  }
  if (l.installed) return [toggle(l.name + "을(를) 켰습니다", { action: "add", id: l.id }, "켜기", "primary")];
  return [toggle(l.name + "을(를) 받았습니다", { action: "add", id: l.id }, working ? "받는 중…" : "받기 · " + mb(l.size), "primary")];
}

// Turning a language on or off is undone by the opposite.
const undoOf = (body) => ({ path: LANGUAGES, body: { action: body.action === "add" ? "remove" : "add", id: body.id } });

function langRow(l, position) {
  const on = position != null, working = pending === "lang-" + l.id;
  const status = on ? (l.installed ? "켜짐 · " + l.version : "팩이 없습니다") : (l.installed ? "받아 둠 · " + l.version : "받을 수 있음");
  const needs = l.requiredBy.length ? " · " + l.requiredBy.join(", ") + " 언어가 이 데이터를 함께 씁니다" : "";
  return h("div", { class: "row", "aria-busy": working },
    h("div", { class: "rank g-" + l.id + (on ? "" : " off"), "aria-hidden": "true" }, GLYPH[l.id] || "·"),
    h("div", {},
      h("div", { class: "name" }, on ? h("span", { class: "pill" }, "#" + (position + 1)) : null, l.name, h("small", {}, l.id)),
      h("div", { class: "meta" }, status + needs)),
    h("div", { class: "actions" }, langActions(l, position)));
}

function renderLang() {
  const p = $("p-lang");
  p.replaceChildren();
  if (state.catalogError) p.append(h("div", { class: "banner bad", role: "alert" }, "릴리스 목록을 불러오지 못해서 설치된 언어만 보입니다. " + state.catalogError));
  for (const why of state.problems) p.append(h("div", { class: "banner bad", role: "alert" }, "쓸 수 없는 팩: " + why));

  const flow = h("div", { class: "flow" });
  flow.append(
    h("span", { class: "cap" }, "단어에서 Esc를 누를 때마다 켜진 언어 순서대로 읽기가 바뀝니다. 순서는 아래 ↑↓로 바꾸고, 하나는 켜 두어야 합니다."),
    h("div", { class: "esc" },
      h("button", { class: "key", type: "button", onclick: pressEsc, "aria-label": "Esc 눌러 보기" }, "Esc"),
      h("span", { class: "typed" }, ESC_WORD),
      h("span", { id: "esc-shown", class: "shown", "aria-live": "polite" }, "…")));
  p.append(flow);

  const byId = Object.fromEntries(state.languages.map((l) => [l.id, l]));
  p.append(h("h2", {}, "켜진 언어"));
  state.order.forEach((id, i) => { if (byId[id]) p.append(langRow(byId[id], i)); });

  const rest = state.languages.filter((l) => !state.order.includes(l.id));
  p.append(h("h2", {}, "더 쓸 수 있는 언어"));
  if (!rest.length) p.append(h("div", { class: "emptybox" }, h("b", {}, "받을 수 있는 언어를 모두 켰습니다"), "새 언어는 릴리스에 올라오면 여기에 나타납니다. 업데이트 탭에서 확인해 보세요."));
  rest.forEach((l) => p.append(langRow(l, null)));
  loadEsc();
}
