"use strict";
// 삭제 (in the 업데이트 tab): takes Typescript and what it learned off this computer. Never one
// click: the first button only opens a question, and the answer is a button that has to be held
// down for HOLD_MS, filling as it goes. Letting go, moving off it, or losing the focus starts over.

const HOLD_MS = 1500;
const removal = { asking: false, done: "", error: "" };   // the question is open / the server's answer

// A button that does its thing only when it has been held for HOLD_MS (mouse, touch or, with the
// keyboard, Space or Enter). `onDone` runs once, when it is full.
function holdButton(label, onDone) {
  const fill = h("span", { class: "fill", "aria-hidden": "true" });
  // The label twice: light on the dark part, dark on the filled part (that copy is cut to the fill's width).
  const btn = h("button", { type: "button", class: "btn hold", "data-f": "hold-delete", "aria-label": label + " (1.5초 동안 누르고 계세요)" },
    fill, h("span", { class: "label" }, label), h("span", { class: "label over", "aria-hidden": "true" }, label));
  let active = false, raf = 0, start = 0;
  const show = (share) => {
    fill.style.transform = "scaleX(" + share + ")";   // drawn here, not by a CSS transition: it also fills when motion is reduced
    btn.style.setProperty("--p", share * 100 + "%");
  };
  const reset = () => {
    active = false;
    cancelAnimationFrame(raf);
    show(0);
    btn.classList.remove("holding");
  };
  const tick = () => {
    if (!active) return;
    const share = Math.min((performance.now() - start) / HOLD_MS, 1);
    show(share);
    if (share >= 1) { active = false; btn.classList.add("full"); onDone(); return; }
    raf = requestAnimationFrame(tick);
  };
  const begin = () => {
    if (active || btn.disabled) return;
    active = true; start = performance.now();
    btn.classList.add("holding");
    raf = requestAnimationFrame(tick);
  };
  btn.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    // Captured, so the release is seen wherever it happens; without it (the pointer is already
    // gone) the move check below still cancels.
    try { btn.setPointerCapture(e.pointerId); } catch (err) { /* not captured */ }
    begin();
  });
  btn.addEventListener("pointermove", (e) => {   // captured, so "leaving" is checked by hand
    const r = btn.getBoundingClientRect();
    if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) reset();
  });
  for (const type of ["pointerup", "pointercancel", "lostpointercapture", "blur"]) btn.addEventListener(type, reset);
  btn.addEventListener("keydown", (e) => { if ((e.key === " " || e.key === "Enter") && !e.repeat) { e.preventDefault(); begin(); } });
  btn.addEventListener("keyup", (e) => { if (e.key === " " || e.key === "Enter") reset(); });
  btn.addEventListener("click", (e) => e.preventDefault());
  btn.addEventListener("contextmenu", (e) => e.preventDefault());
  return btn;
}

async function confirmRemoval() {
  if (busy) return;
  busy = true; removal.error = "";
  renderUpdate();
  try {
    removal.done = (await api("/api/uninstall", { confirm: "delete" })).outcome;
    removal.asking = false;
    try { await api("/api/quit", {}); } catch (e) { /* it ends by itself */ }
  } catch (e) { removal.error = e.message; }
  busy = false;
  renderUpdate();
}

// The section: what would go, then the question, then (once asked) what happened.
function uninstallSection() {
  const info = state.uninstall || { program: false, testHome: false, targets: [] };
  const box = h("div", { class: "card pad" + (removal.asking ? " danger" : "") });
  if (removal.done) {
    box.append(h("b", {}, removal.done === "installer" ? "삭제를 시작했습니다" : "삭제했습니다"),
      h("p", { class: "note", role: "status" }, removal.done === "installer"
        ? "관리자 권한을 묻는 창이 뜨면 ‘예’를 눌러 주세요. 이 창은 곧 닫힙니다. 열려 있는 앱은 다시 시작해야 입력기가 완전히 사라집니다."
        : "설정, 배운 단어, 언어 팩을 지웠습니다. 프로그램은 그대로 두었습니다."));
    return [h("h2", {}, "삭제"), box];
  }
  box.append(h("p", { class: "note" },
    info.program ? "입력기와 프로그램을 지우고, 배운 단어·설정·언어 팩도 모두 지웁니다. 관리자 권한 확인이 한 번 나옵니다."
      : "배운 단어, 설정, 언어 팩을 지웁니다." + (info.testHome ? " (시험용 폴더라서 설치된 프로그램은 건드리지 않습니다.)" : " 이 창은 설치된 프로그램이 아니라서 프로그램은 그대로 둡니다.")));
  if (info.targets.length) box.append(h("ul", { class: "targets" }, ...info.targets.map((t) => h("li", {}, t))));
  else box.append(h("p", { class: "note" }, "지울 데이터가 없습니다."));
  if (removal.error) box.append(h("p", { class: "note", role: "alert" }, removal.error));
  if (!removal.asking) {
    box.append(h("button", { type: "button", class: "btn danger", "data-f": "ask-delete", disabled: busy, onclick: () => { removal.asking = true; renderUpdate(); } }, "삭제…"));
  } else {
    box.append(h("p", { class: "ask" }, h("b", {}, "정말 지울까요?"), " 되돌릴 수 없습니다. 아래 버튼을 1.5초 동안 꾹 누르고 계세요."),
      h("div", { class: "removal-choices" },
        busy ? h("span", { class: "note", role: "status" }, "지우는 중…") : holdButton("꾹 눌러서 삭제", confirmRemoval),
        h("button", { type: "button", class: "btn quiet", disabled: busy, onclick: () => { removal.asking = false; renderUpdate(); } }, "취소")));
  }
  return [h("h2", {}, "삭제"), box];
}
