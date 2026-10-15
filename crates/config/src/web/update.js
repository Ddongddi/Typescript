// 업데이트 tab: the version, and a check for a newer release (the program and the language packs).

const updates = { info: null, reopen: false, log: null, newer: null, fold: null }; // GET /api/update, once asked; whether the program was just replaced

function updateButtonText(info) {
  if (!info.coreChanged) return "언어 팩 업데이트";
  return state.os === "windows" ? "업데이트 (관리자 확인창이 한 번 뜹니다)" : "업데이트";
}

// `code` and **bold** of a changelog line, as nodes (the text itself never becomes markup).
function inline(text) {
  return text
    .split(/(`[^`]+`|\*\*[^*]+\*\*)/)
    .filter(Boolean)
    .map((part) =>
      part.startsWith("`")
        ? h("code", {}, part.slice(1, -1))
        : part.startsWith("**")
          ? h("b", {}, part.slice(2, -2))
          : part,
    );
}

// Adds one changelog line to a version: a "### group" starts a group, "- item" an item (in a
// group of no title when the version has none), an indented line continues the item above, and
// a plain paragraph before the first group is the version's description.
function takeLine(now, line) {
  if (line.startsWith("### ")) now.groups.push({ title: line.slice(4).trim(), items: [] });
  else if (line.startsWith("- ")) {
    if (!now.groups.length) now.groups.push({ title: "", items: [] });
    now.groups[now.groups.length - 1].items.push(line.slice(2));
  } else if (/^\s+\S/.test(line) && now.groups.length) {
    const items = now.groups[now.groups.length - 1].items;
    if (items.length) items[items.length - 1] += " " + line.trim();
  } else if (line.trim() && !now.groups.length) {
    now.intro.push(line.trim());
  }
}

// CHANGELOG.md as a list of versions ("## [x.y.z] - date" or "## x.y.z", then a description,
// groups and items).
function parseChangelog(text) {
  const versions = [];
  for (const line of text.split(/\r?\n/)) {
    const head = line.match(/^## \[?([^\]\s]+)\]?(?: - (\S+))?/);
    if (head) {
      versions.push({ version: head[1], date: head[2] || "", intro: [], groups: [] });
      continue;
    }
    if (/^\[[^\]]+\]: /.test(line)) break; // the link list at the end of the file
    if (versions.length) takeLine(versions[versions.length - 1], line);
  }
  return versions.filter((v) => v.groups.length || v.intro.length);
}

// A release's notes (the text GitHub holds for it) as the same kind of version.
function parseNotes(release) {
  const now = { version: release.version, date: release.date, intro: [], groups: [] };
  for (const line of release.notes.split(/\r?\n/)) takeLine(now, line);
  return now;
}

function versionFold(v, open, tag) {
  return h(
    "details",
    { open },
    h(
      "summary",
      {},
      h("b", {}, v.version === "Unreleased" ? "다음 버전" : v.version),
      h("span", {}, v.date),
      tag ? h("em", {}, tag) : "",
    ),
    h(
      "div",
      { class: "body" },
      ...(v.intro || []).map((p) => h("p", { class: "note" }, ...inline(p))),
      ...v.groups.flatMap((g) => [
        g.title ? h("h4", {}, g.title) : null,
        h("ul", {}, ...g.items.map((it) => h("li", {}, ...inline(it)))),
      ]),
    ),
  );
}

// The whole history: versions newer than this one (from GitHub, once a check found them), then
// every version this build knows, newest first. `updates.fold` is null (each as it is), or
// true/false after "모두 펼치기" / "모두 접기".
function changelog(text) {
  const mine = parseChangelog(text);
  const newer = (updates.newer || [])
    .map(parseNotes)
    .filter((v) => v.groups.length || v.intro.length);
  const current = (v, i) =>
    v.version === state.version ||
    (i === 0 && v.version !== "Unreleased" && !mine.some((x) => x.version === state.version));
  const open = (def) => (updates.fold === null ? def : updates.fold);
  const count = mine.filter((v) => v.version !== "Unreleased").length + newer.length;
  return h(
    "div",
    {},
    h(
      "div",
      { class: "logbar" },
      h("span", { class: "note" }, `${count}개 버전의 변경 기록`),
      h(
        "button",
        {
          class: "btn quiet",
          type: "button",
          "data-f": "log-open",
          onclick: () => {
            updates.fold = true;
            renderUpdate();
          },
        },
        "모두 펼치기",
      ),
      h(
        "button",
        {
          class: "btn quiet",
          type: "button",
          "data-f": "log-close",
          onclick: () => {
            updates.fold = false;
            renderUpdate();
          },
        },
        "모두 접기",
      ),
    ),
    h(
      "div",
      { class: "log" },
      ...newer.map((v) => versionFold(v, open(true), "새 버전")),
      ...mine.map((v, i) =>
        versionFold(v, open(current(v, i)), v.version === state.version ? "지금 버전" : ""),
      ),
    ),
  );
}

async function loadChangelog() {
  if (updates.log !== null) return;
  updates.log = ""; // asked once
  try {
    updates.log = (await api("/api/changelog")).text;
  } catch (e) {
    updates.log = "";
  }
  // Draw again whatever tab is in front: the request usually lands while another tab is open,
  // and opening this tab later does not draw again by itself.
  renderUpdate();
}

function renderUpdate() {
  const p = $("p-upd"),
    info = updates.info,
    working = busy && pending === "upd";
  p.replaceChildren(
    ...(updates.reopen
      ? [
          h(
            "div",
            { class: "banner", role: "status" },
            "설치가 끝나면 이 설정 창을 닫고 터미널에서 typescript-config 를 다시 여세요. 지금 보이는 화면은 이전 버전입니다.",
          ),
        ]
      : []),
    h("h2", {}, "현재 버전"),
    h(
      "div",
      { class: "card pad" },
      h("div", {}, h("b", {}, "Typescript " + state.version)),
      h(
        "p",
        { class: "note ver-link", style: "margin:6px 0 0" },
        "github | ",
        h(
          "a",
          {
            href: "https://github.com/iamallyouneed/Typescript",
            target: "_blank",
            rel: "noopener noreferrer",
          },
          "iamallyouneed/Typescript",
        ),
      ),
    ),
    h("h2", {}, "새 버전"),
  );
  const box = h("div", { class: "card pad", "aria-busy": working });
  if (!info)
    box.append(h("p", { class: "note" }, "GitHub 릴리스에서 새 버전과 언어 팩 갱신을 확인합니다."));
  else if (info.error) box.append(h("p", { class: "note", role: "alert" }, info.error));
  else if (!info.available) box.append(h("p", { class: "note" }, "최신 버전입니다."));
  else {
    box.append(h("b", {}, info.latest + " 이(가) 나왔습니다"));
    if (info.notes) box.append(h("pre", { class: "note" }, info.notes));
    box.append(
      h(
        "button",
        { class: "btn primary", disabled: busy, onclick: applyUpdate },
        updateButtonText(info),
      ),
    );
  }
  box.append(
    h(
      "div",
      { style: "margin-top:14px" },
      h(
        "button",
        { class: "btn", "data-f": "upd-check", disabled: busy, onclick: checkUpdate },
        working ? "확인 중…" : "지금 확인",
      ),
    ),
  );
  p.append(box, ...uninstallSection());
  if (updates.log) p.append(h("h2", {}, "변경 기록"), changelog(updates.log));
  else loadChangelog();
}

async function checkUpdate() {
  busy = true;
  pending = "upd";
  render("upd-check");
  try {
    updates.info = await api("/api/update");
  } catch (e) {
    updates.info = { error: e.message };
  }
  // Versions newer than this build are not in its changelog: ask GitHub for their notes.
  if (updates.info && updates.info.coreChanged) {
    try {
      updates.newer = (await api("/api/releases")).releases;
    } catch (e) {
      updates.newer = [];
    }
  }
  busy = false;
  pending = null;
  render("upd-check");
}

async function applyUpdate() {
  busy = true;
  pending = "upd";
  render();
  try {
    const programChanged = updates.info && updates.info.coreChanged;
    const r = await api("/api/update", { action: "apply" });
    toast(
      programChanged
        ? "업데이트를 시작했습니다. 끝나면 설정을 다시 여세요"
        : r.message || "업데이트했습니다",
    );
    updates.reopen = !!programChanged;
    updates.info = null;
    state = await api("/api/state");
  } catch (e) {
    toast(e.message, true);
  }
  busy = false;
  pending = null;
  render();
}
