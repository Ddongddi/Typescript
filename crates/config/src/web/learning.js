"use strict";
// What the input method has learned, and a way to forget it: the words taught by correcting them
// twice (shown on the 언어 tab) and the words Tab completion remembers (the 입력 설정 tab).
// Wiping asks twice (the button turns into "정말 지울까요?") instead of opening a dialog.

const learning = {
  data: null,
  loading: false,
  error: "",
  sure: null,
  query: "",
  page: 0,
  request: 0,
};
let learningTimer;

function renderLearning() {
  if (state && (!$("p-lang").hidden || !$("p-set").hidden)) render();
}

async function loadLearning() {
  clearTimeout(learningTimer);
  const request = ++learning.request;
  learning.loading = true;
  learning.error = "";
  renderLearning();
  try {
    const data = await api("/api/learning", {
      action: "list",
      query: learning.query,
      page: learning.page,
    });
    if (request !== learning.request) return;
    learning.data = data;
    learning.page = data.page;
  } catch (e) {
    if (request !== learning.request) return;
    learning.error = e.message;
  }
  if (request !== learning.request) return;
  learning.loading = false;
  renderLearning();
}

async function changeLearning(body, message) {
  if (busy) return;
  busy = true;
  clearTimeout(learningTimer);
  ++learning.request;
  learning.loading = false;
  learning.sure = null;
  render();
  try {
    learning.data = await api("/api/learning", {
      ...body,
      query: learning.query,
      page: learning.page,
    });
    learning.page = learning.data.page;
    learning.error = "";
    toast(message);
  } catch (e) {
    toast(e.message, true);
  }
  busy = false;
  render();
}

function learningStatus(box) {
  if (learning.error) {
    box.append(
      h(
        "p",
        { class: "note", role: "alert" },
        "학습 정보를 불러오지 못했습니다. " + learning.error,
      ),
      h(
        "button",
        { class: "btn", type: "button", disabled: learning.loading || busy, onclick: loadLearning },
        "다시 시도",
      ),
    );
    return true;
  }
  if (!learning.data) {
    box.append(h("p", { class: "note", role: "status" }, "불러오는 중…"));
    return true;
  }
  return false;
}

// A destructive button that must be pressed twice.
function wipeButton(key, label, run) {
  const asked = learning.sure === key;
  return h(
    "button",
    {
      class: "btn " + (asked ? "primary" : "quiet"),
      type: "button",
      "data-f": "wipe-" + key,
      disabled: busy,
      onclick: () => {
        if (asked) run();
        else {
          learning.sure = key;
          render("wipe-" + key);
        }
      },
    },
    asked ? "정말 지울까요?" : label,
  );
}

// The 언어 tab: words taught by correcting them the same way twice.
function taughtWords() {
  const d = learning.data;
  const box = h("div", { class: "card pad" });
  if (learningStatus(box)) {
    return [h("h2", {}, "가르친 단어"), box];
  }
  box.append(
    h("input", {
      type: "text",
      "data-f": "learned-filter",
      value: learning.query,
      placeholder: "입력 키로 전체 학습 단어 검색",
      "aria-label": "가르친 단어 검색",
      maxlength: "200",
      disabled: busy,
      oninput: (e) => {
        learning.query = e.target.value;
        learning.page = 0;
        ++learning.request;
        clearTimeout(learningTimer);
        learningTimer = setTimeout(loadLearning, 250);
      },
    }),
    h(
      "p",
      { class: "note" },
      `같은 교정(Esc)을 ${d.suggestAfter}번 하면 그 단어를 배웁니다. 여기서 지우면 다시 입력기가 스스로 판단합니다.`,
    ),
  );
  if (!d.learned.length) {
    box.append(
      h(
        "p",
        { class: "note" },
        learning.query
          ? "검색에 맞는 학습 단어가 없습니다."
          : d.pendingCount
            ? `아직 배운 단어는 없습니다. 한 번 교정한 단어 ${d.pendingCount}개는 한 번 더 같게 고치면 배웁니다.`
            : "아직 배운 단어가 없습니다.",
      ),
    );
  } else {
    box.append(
      h(
        "ul",
        { class: "taught" },
        ...d.learned.map((w) =>
          h(
            "li",
            {},
            h("code", {}, w.keys),
            h("span", {}, "→ " + (w.out || langName(w.language))),
            h(
              "button",
              {
                class: "btn quiet",
                type: "button",
                disabled: busy,
                "aria-label": w.keys + " 잊기",
                onclick: () => changeLearning({ action: "forget", keys: w.keys }, "잊었습니다"),
              },
              "삭제",
            ),
          ),
        ),
      ),
    );
    if (d.pages > 1)
      box.append(
        h(
          "p",
          { class: "note" },
          `검색 결과 ${d.filteredCount}개 · ${d.page + 1} / ${d.pages} 페이지`,
        ),
      );
  }
  if (d.pages > 1) {
    const go = (delta) => {
      learning.page += delta;
      loadLearning();
    };
    box.append(
      h(
        "div",
        { class: "pager" },
        h(
          "button",
          {
            class: "btn",
            disabled: busy || learning.loading || d.page === 0,
            onclick: () => go(-1),
          },
          "이전",
        ),
        h("span", { role: "status" }, `${d.page + 1} / ${d.pages}`),
        h(
          "button",
          {
            class: "btn",
            disabled: busy || learning.loading || d.page + 1 >= d.pages,
            onclick: () => go(1),
          },
          "다음",
        ),
      ),
    );
  }
  if (d.learnedCount || d.pendingCount) {
    box.append(
      h(
        "p",
        { class: "note" },
        `배운 단어 ${d.learnedCount}개` +
          (d.pendingCount ? ` · 한 번 교정한 단어 ${d.pendingCount}개` : ""),
      ),
    );
    box.append(
      wipeButton("taught", "모두 지우기", () =>
        changeLearning({ action: "forgetAll" }, "배운 단어를 모두 지웠습니다"),
      ),
    );
  }
  return [h("h2", {}, "가르친 단어"), box];
}

// The 입력 설정 tab: the words Tab completion counted.
function wordHistory() {
  const d = learning.data;
  const box = h("div", { class: "card pad" });
  if (learningStatus(box)) {
    return [h("h2", {}, "단어 기록"), box];
  }
  box.append(
    h(
      "p",
      { class: "note" },
      `Tab 자동완성은 ${d.suggestAfter}번 이상 친 단어만 제안합니다. 한 번만 친 단어(비밀번호나 오타)는 제안하지 않습니다. 비밀번호 칸에서는 아예 기록하지 않습니다.`,
    ),
    h(
      "p",
      { class: "note" },
      `기억하는 단어 ${d.historyWords}개 · 그중 제안하는 단어 ${d.historySuggestible}개. 한자 변환은 읽기마다 최근에 고른 ${d.kanjiPerReading}개를 기억합니다.`,
    ),
  );
  if (d.historyWords)
    box.append(
      wipeButton("history", "기록 지우기", () =>
        changeLearning({ action: "clearHistory" }, "단어 기록을 지웠습니다"),
      ),
    );
  return [h("h2", {}, "단어 기록"), box];
}
