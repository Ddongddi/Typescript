// Draws everything from `state`, switches tabs, listens for shortcuts, and starts the page.

const TABS = ["lang", "try", "set", "apps", "upd"];
const tabOf = (name) => $("t-" + name), panelOf = (name) => $("p-" + name);

// Draws every panel again. `focusKey` (or the focused control's data-f) gets the focus back.
function render(focusKey) {
  if (!state) return;
  openDd = null;
  $("ver").textContent = "v" + state.version;
  const active = focusKey || (document.activeElement && document.activeElement.dataset && document.activeElement.dataset.f);
  renderLang(); renderSettings(); renderApps(); renderUpdate(); updateTry();
  if (active) {
    const el = document.querySelector('[data-f="' + CSS.escape(active) + '"]');
    if (el && !el.disabled) el.focus({ preventScroll: true });
  }
}

function selectTab(index, focus) {
  TABS.forEach((name, k) => {
    tabOf(name).setAttribute("aria-selected", k === index);
    tabOf(name).tabIndex = k === index ? 0 : -1;
    panelOf(name).hidden = k !== index;
  });
  if (focus) tabOf(TABS[index]).focus();
  history.replaceState(null, "", "/#" + TABS[index]);
  if (TABS[index] === "try" && state) runTry();
  if (TABS[index] === "apps" && running.list === null) loadRunning();
}

TABS.forEach((name, i) => {
  tabOf(name).addEventListener("click", () => selectTab(i, true));
  tabOf(name).addEventListener("keydown", (e) => {
    const to = { ArrowRight: (i + 1) % TABS.length, ArrowLeft: (i + TABS.length - 1) % TABS.length, Home: 0, End: TABS.length - 1 }[e.key];
    if (to !== undefined) { e.preventDefault(); selectTab(to, true); }
  });
});

// 1-5 switch tabs and / finds an app, wherever the focus is except in a text field. (On the
// 체험 tab every key types into the box; Esc leaves it.)
document.addEventListener("keydown", (e) => {
  if (e.ctrlKey || e.metaKey || e.altKey || editable(e.target) || openDd || !panelOf("try").hidden) return;
  const tab = Number(e.key) - 1;
  if (tab >= 0 && tab < TABS.length) { e.preventDefault(); selectTab(tab, false); }
  else if (e.key === "/") {
    e.preventDefault();
    selectTab(TABS.indexOf("apps"), false);
    const box = document.querySelector('[data-f="app-filter"]');
    if (box) box.focus();
  }
});

async function heartbeat() {
  try {
    await api("/api/heartbeat", {});
    $("dot").className = "dot";
    $("linktext").textContent = "연결됨";
  } catch (e) {
    $("dot").className = "dot off";
    $("linktext").textContent = "끊어짐 — 터미널에서 다시 여세요";
  }
}
setInterval(heartbeat, 5000);

(async () => {
  buildTry();
  const start = TABS.indexOf(location.hash.slice(1));
  if (start > 0) selectTab(start, false);
  try {
    state = await api("/api/state");
    render();
    if (!panelOf("apps").hidden) loadRunning();
  } catch (e) {
    $("p-lang").replaceChildren(h("div", { class: "banner bad", role: "alert" }, e.message + " 터미널에서 typescript-config 를 다시 실행해 주세요."));
  }
})();
