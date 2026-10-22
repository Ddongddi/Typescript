// Draws everything from `state`, switches tabs, listens for shortcuts, and starts the page.

const TABS = ["lang", "set", "keys", "apps", "upd"];
$("copy-diagnostics").addEventListener("click", async () => {
  const button = $("copy-diagnostics");
  button.disabled = true;
  try {
    const text = JSON.stringify(await api("/api/diagnostics"), null, 2);
    $("diagnostics-text").value = text;
    $("diagnostics").hidden = false;
    try {
      await navigator.clipboard.writeText(text);
      toast("진단 정보를 복사했습니다. 문의할 때 함께 보내 주세요.");
    } catch (e) {
      $("diagnostics").open = true;
      $("diagnostics-text").focus();
      $("diagnostics-text").select();
      toast("복사 권한이 없습니다. 선택된 진단 정보를 직접 복사해 주세요.", true);
    }
  } catch (e) {
    toast("진단 정보를 불러오지 못했습니다. " + e.message, true);
  } finally {
    button.disabled = false;
  }
});
const tabOf = (name) => $("t-" + name),
  panelOf = (name) => $("p-" + name);

// Draws the visible panel; hidden panels use the latest state when opened.
function render(focusKey) {
  if (!state) return;
  closeDd(false);
  $("ver").textContent = "v" + state.version;
  $("save-status").textContent = busy
    ? "처리 중… 완료될 때까지 기다려 주세요."
    : "변경사항은 바로 저장됩니다.";
  const active =
    focusKey ||
    (document.activeElement && document.activeElement.dataset && document.activeElement.dataset.f);
  const selection = document.activeElement && document.activeElement.selectionStart;
  const name = TABS.find((tab) => !panelOf(tab).hidden);
  ({
    lang: renderLang,
    set: renderSettings,
    keys: renderKeys,
    apps: renderApps,
    upd: renderUpdate,
  })[name]();
  if (!$("chat-in")) renderChat();
  if (active) {
    const el = document.querySelector('[data-f="' + CSS.escape(active) + '"]');
    if (el && !el.disabled) {
      el.focus({ preventScroll: true });
      if (selection != null && el.type === "text") el.setSelectionRange(selection, selection);
    }
  }
}

// Draws only the keys panel. Key designation is all local until 지정 is pressed, so rebuilding
// the whole page (chat included) on every keypress flickers for nothing.
function renderKeysPanel(focusKey) {
  if (!state) return;
  renderKeys();
  if (focusKey) {
    const el = document.querySelector('[data-f="' + CSS.escape(focusKey) + '"]');
    if (el && !el.disabled) el.focus({ preventScroll: true });
  }
}

function selectTab(index, focus) {
  stopCapture();
  stopHangulCapture();
  // The designation panel listens only while the 키 tab is in front. Leaving closes it (the draft
  // is dropped) and draws the tab again, so what is shown is what is held.
  if (assignOpen && TABS[index] === "keys") startCapture();
  else if (assignOpen) {
    stopAssign();
    renderKeysPanel();
  }
  TABS.forEach((name, k) => {
    tabOf(name).setAttribute("aria-selected", k === index);
    tabOf(name).tabIndex = k === index ? 0 : -1;
    panelOf(name).hidden = k !== index;
  });
  if (focus) tabOf(TABS[index]).focus();
  history.replaceState(null, "", "/#" + TABS[index]);
  render();
  if (TABS[index] === "apps" && running.list === null) loadRunning();
}

TABS.forEach((name, i) => {
  tabOf(name).addEventListener("click", () => selectTab(i, true));
  tabOf(name).addEventListener("keydown", (e) => {
    const to = {
      ArrowRight: (i + 1) % TABS.length,
      ArrowLeft: (i + TABS.length - 1) % TABS.length,
      Home: 0,
      End: TABS.length - 1,
    }[e.key];
    if (to !== undefined) {
      e.preventDefault();
      selectTab(to, true);
    }
  });
});

// 1-5 switch tabs and / finds an app, wherever the focus is except in a text field (the chat
// box on the right is one). Other letters typed on a blank part of the page go to the chat.
document.addEventListener("keydown", (e) => {
  if (e.ctrlKey || e.metaKey || e.altKey || editable(e.target) || openDd) return;
  const tab = Number(e.key) - 1;
  if (tab >= 0 && tab < TABS.length) {
    e.preventDefault();
    selectTab(tab, false);
  } else if (e.key === "/") {
    e.preventDefault();
    selectTab(TABS.indexOf("apps"), false);
    const box = document.querySelector('[data-f="app-filter"]');
    if (box) box.focus();
  } else typeIntoChat(e); // anything else typed on a blank part of the page is for the chat
});

// How to switch input methods, per system, and the steps that make tacit appear in the list
// at all (the part people get stuck on right after installing).
const SWITCH_KEY = {
  windows: "Win+Space",
  macos: "Control+Space (또는 메뉴 막대의 입력 소스)",
  linux: "Super+Space (또는 상단 메뉴의 입력 소스)",
};
const SETUP_STEPS = {
  windows: [],
  macos: [
    "로그아웃했다가 다시 로그인합니다.",
    "시스템 설정 → 키보드 → 입력 소스 → 편집에서 Tacit을 추가합니다.",
    "서명하지 않은 앱이라 처음에는 개인정보 보호 및 보안에서 허용해야 할 수 있습니다.",
    "메뉴 막대의 입력 소스에서 Tacit을 고릅니다.",
  ],
  linux: [
    "설정 → 키보드 → 입력 소스(또는 ibus-setup)에서 Tacit을 추가합니다.",
    "목록에 없으면 로그아웃했다가 다시 로그인합니다.",
    "상단 메뉴의 입력 소스에서 Tacit을 고릅니다.",
  ],
};

let setupDismissed = false;
try {
  setupDismissed = localStorage.getItem("ts-setup-done") === "1";
} catch (e) {
  /* private window: shown every time */
}

// The "after installing" checklist, where the system cannot say which input method is selected
// (macOS and Linux answer only when their tools do): shown until it is dismissed.
function showSetup(steps) {
  const box = $("setup");
  box.hidden = !steps.length || setupDismissed;
  if (box.hidden) return;
  box.replaceChildren(
    h("b", {}, "설치 후 한 번만 해 주세요"),
    h("ol", {}, ...steps.map((s) => h("li", {}, s))),
    h(
      "button",
      {
        class: "btn quiet",
        type: "button",
        onclick: () => {
          setupDismissed = true;
          try {
            localStorage.setItem("ts-setup-done", "1");
          } catch (e) {
            /* stays for this page */
          }
          box.hidden = true;
        },
      },
      "마쳤어요",
    ),
  );
}

// Tells when the input method selected is not tacit (the heartbeat answers it).
function showIme(ime) {
  const banner = $("ime"),
    os = state ? state.os : "";
  if (!ime) {
    banner.hidden = true;
    showSetup(SETUP_STEPS[os] || []);
    return;
  }
  showSetup([]);
  banner.hidden = ime.tacit;
  if (banner.hidden) return;
  const name = ime.name ? `‘${ime.name}’` : "다른 입력기";
  banner.textContent = `지금 선택된 입력기는 ${name}입니다. Tacit이 아니라서 여기서 정한 대로 동작하지 않아요. ${SWITCH_KEY[os] || "입력기 전환 키"}로 Tacit을 고르세요.`;
}

async function heartbeat() {
  try {
    const answer = await api("/api/heartbeat", {});
    $("dot").className = "dot";
    $("linktext").textContent = "연결됨";
    showIme(answer && answer.ime);
    return true;
  } catch (e) {
    $("dot").className = "dot off";
    $("linktext").textContent = "끊어짐 — 터미널에서 다시 여세요";
    return false;
  }
}

// Every 5 s while the page is in front, every 30 s behind another window (the server ends on
// its own after 3 minutes of silence, so it must never stop altogether), and slower while it
// keeps failing.
let beatDelay = 5000;
async function beat() {
  const ok = await heartbeat();
  beatDelay = ok ? (document.hidden ? 30000 : 5000) : Math.min(beatDelay * 2, 30000);
  setTimeout(beat, beatDelay);
}
beat();
window.addEventListener("focus", heartbeat); // back from switching the input method
document.addEventListener("visibilitychange", () => {
  if (!document.hidden) heartbeat();
});

$("test-jump").addEventListener("click", () => {
  const box = $("chat-in");
  if (box) {
    box.scrollIntoView({ block: "center" });
    box.focus({ preventScroll: true });
  }
});

async function loadState() {
  const error = $("load-error");
  error.hidden = true;
  try {
    state = await api("/api/state");
    render();
    loadLearning();
    if (!panelOf("apps").hidden) loadRunning();
  } catch (e) {
    error.replaceChildren(
      h("p", {}, "설정을 불러오지 못했습니다. " + e.message),
      h("button", { class: "btn", type: "button", onclick: loadState }, "다시 시도"),
    );
    error.hidden = false;
  }
}

(() => {
  const start = TABS.indexOf(location.hash.slice(1));
  if (start > 0) selectTab(start, false);
  loadState();
})();
