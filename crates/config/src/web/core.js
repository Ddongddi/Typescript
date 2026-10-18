"use strict";
// What every panel shares: the token, the server state, a tiny DOM builder, the toast, `act`.
// All scripts are one scope (see web.rs), so what is declared here is visible everywhere.

const params = new URLSearchParams(location.search);
let token = params.get("t") || sessionStorage.getItem("ts-token") || "";
if (params.get("t")) {
  sessionStorage.setItem("ts-token", token);
  history.replaceState(null, "", location.pathname + location.hash);
}

let state = null; // GET /api/state, replaced by every change
let busy = false; // a change is on its way: controls are disabled
let pending = null; // which control asked for it (shows "받는 중…" and the like)

const $ = (id) => document.getElementById(id);
const GLYPH = { en: "A", ko: "가", ja: "あ" };
const mb = (n) => (n / 1048576).toFixed(1) + " MB";
const langName = (id) =>
  ((state && state.languages.find((l) => l.id === id)) || {}).name ||
  (id === "mixed" ? "혼합" : id);
// A field keys are typed into: the chat boxes are divs with role=textbox.
const editable = (el) =>
  el &&
  (["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName) ||
    el.isContentEditable ||
    (el.getAttribute && el.getAttribute("role") === "textbox"));
const copy = (value) => JSON.parse(JSON.stringify(value));

// Builds an element. Every value from the server goes in as text, never as markup.
function h(tag, attrs, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (k === "class") el.className = v;
    else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k.startsWith("aria-") && typeof v === "boolean")
      el.setAttribute(k, String(v)); // "true"/"false", not empty
    else if (v === true) el.setAttribute(k, "");
    else if (v !== false && v != null) el.setAttribute(k, v);
  }
  for (const kid of kids.flat())
    if (kid != null) el.append(kid.nodeType ? kid : document.createTextNode(String(kid)));
  return el;
}

// A short message at the bottom. `action` ({label, run}) adds a button, e.g. "되돌리기".
let toastTimer;
function toast(text, bad, action) {
  const t = $("toast");
  t.replaceChildren(text);
  if (action)
    t.append(
      h(
        "button",
        {
          type: "button",
          onclick: () => {
            t.className = "toast";
            action.run();
          },
        },
        action.label,
      ),
    );
  t.className = "toast show" + (bad ? " bad" : "");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(
    () => {
      t.className = "toast";
    },
    bad || action ? 6000 : 2500,
  );
}

// The address carries a token that is new every time tacit-config starts: a bookmarked or
// old page is refused (401). Say so, and what to do, instead of a bare error on every click.
function showExpired() {
  const banner = $("expired");
  banner.textContent =
    "이 창은 더 이상 쓸 수 없습니다. 설정 앱이 다시 시작되었거나 주소가 오래되었습니다. 시작 메뉴의 ‘tacit 설정’(또는 터미널에서 tacit-config)을 다시 열어 주세요.";
  banner.hidden = false;
}

async function api(path, body) {
  const res = await fetch(path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "X-Token": token,
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  let data = {};
  try {
    data = await res.json();
  } catch (e) {
    /* not JSON */
  }
  if (res.status === 401) showExpired();
  if (!res.ok) throw new Error(data.error || "요청이 거부되었습니다 (" + res.status + ")");
  return data;
}

// Changes something on the server and draws the page again. `key` marks the control to give
// focus back to. `undo` ({path, body}) is the request that puts it back: the toast offers it.
async function act(label, path, body, key, undo) {
  if (busy) return;
  busy = true;
  pending = key || null;
  render(key);
  try {
    state = await api(path, body);
    const back = undo && {
      label: "되돌리기",
      run: () => act("되돌렸습니다", undo.path, undo.body, key),
    };
    if (label) toast(label, false, back);
  } catch (e) {
    toast(e.message, true);
  }
  busy = false;
  pending = null;
  render(key);
}
