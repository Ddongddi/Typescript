// 업데이트 tab: the version, and a check for a newer release (the program and the language packs).

const updates = { info: null };   // answer of GET /api/update, once asked

function updateButtonText(info) {
  if (!info.coreChanged) return "언어 팩 업데이트";
  return state.os === "windows" ? "업데이트 (관리자 확인창이 한 번 뜹니다)" : "업데이트";
}

function renderUpdate() {
  const p = $("p-upd"), info = updates.info, working = busy && pending === "upd";
  p.replaceChildren(
    h("h2", {}, "현재 버전"), h("div", { class: "card pad" }, h("b", {}, "Typescript " + state.version)),
    h("h2", {}, "새 버전"));
  const box = h("div", { class: "card pad", "aria-busy": working });
  if (!info) box.append(h("p", { class: "note" }, "GitHub 릴리스에서 새 버전과 언어 팩 갱신을 확인합니다."));
  else if (info.error) box.append(h("p", { class: "note", role: "alert" }, info.error));
  else if (!info.available) box.append(h("p", { class: "note" }, "최신 버전입니다."));
  else {
    box.append(h("b", {}, info.latest + " 이(가) 나왔습니다"));
    if (info.notes) box.append(h("pre", { class: "note" }, info.notes));
    box.append(h("button", { class: "btn primary", disabled: busy, onclick: applyUpdate }, updateButtonText(info)));
  }
  box.append(h("div", { style: "margin-top:14px" },
    h("button", { class: "btn", "data-f": "upd-check", disabled: busy, onclick: checkUpdate }, working ? "확인 중…" : "지금 확인")));
  p.append(box);
}

async function checkUpdate() {
  busy = true; pending = "upd"; render("upd-check");
  try { updates.info = await api("/api/update"); } catch (e) { updates.info = { error: e.message }; }
  busy = false; pending = null; render("upd-check");
}

async function applyUpdate() {
  busy = true; pending = "upd"; render();
  try {
    const r = await api("/api/update", { action: "apply" });
    toast(r.message || "업데이트했습니다");
    updates.info = null;
    state = await api("/api/state");
  } catch (e) { toast(e.message, true); }
  busy = false; pending = null; render();
}
