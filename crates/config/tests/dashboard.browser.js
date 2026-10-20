// Run with playwright-cli run-code --filename crates/config/tests/dashboard.browser.js
// on a dashboard using a disposable TACIT_HOME with word000..word204 learned,
// alpha.exe and beta.exe app rules, and a global conversionThreshold of 0.97.
async function dashboardChecks(page) {
  await page.unrouteAll({ behavior: "ignoreErrors" });
  const check = (ok, message) => {
    if (!ok) throw new Error(message);
  };
  await page.waitForFunction(() => typeof state !== "undefined" && state);
  check(
    await page.evaluate(() => state.uninstall.testHome),
    "use a disposable TACIT_HOME, never a live profile",
  );
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const url = page.url().split("#")[0];
  let previews = 0;
  page.on("request", (request) => {
    if (request.url().endsWith("/api/try")) previews++;
  });

  await page.route(
    "**/api/state",
    (route) => route.fulfill({ status: 500, json: { error: "test state failure" } }),
    { times: 1 },
  );
  await page.goto(url + "#apps");
  await page.reload();
  await page.locator("#load-error").waitFor({ state: "visible" });
  await page.locator("#load-error button").click();
  await page.locator("#p-apps details").first().waitFor();
  check(
    (await page.locator('#p-apps input[type="range"]').inputValue()) === "0.97",
    "inherited slider value",
  );
  check(
    (await page.locator("#p-apps output").first().textContent()).includes("0.97"),
    "inherited value label",
  );

  const draft = page.locator('[data-f="app-name"]');
  await draft.fill("draft.exe");
  await page.locator("#t-set").click();
  const before = previews;
  let releaseSave;
  const saveGate = new Promise((resolve) => {
    releaseSave = resolve;
  });
  await page.route(
    "**/api/settings",
    async (route) => {
      await saveGate;
      await route.continue();
    },
    { times: 1 },
  );
  const toggle = page.locator('[data-f="returnConfirmsOnly"]');
  const oldValue = await toggle.isChecked();
  await toggle.click();
  check(await toggle.isDisabled(), "settings disabled during save");
  check(
    (await page.locator('#p-set input[type="checkbox"]:enabled').count()) === 0,
    "all switches disabled",
  );
  releaseSave();
  await page.waitForFunction(
    () => !document.querySelector('[data-f="returnConfirmsOnly"]').disabled,
  );
  check((await toggle.isChecked()) !== oldValue, "setting saved");
  check(previews === before, "hidden app previews stay idle");
  await page.locator("#t-apps").click();
  check((await draft.inputValue()) === "draft.exe", "draft preserved across tab and save");
  await page.locator('[data-f="rule-filter"]').fill("ALPHA");
  check((await page.locator('details[aria-label="beta.exe"]').count()) === 0, "saved rule search");
  const alpha = page.locator('details[aria-label="alpha.exe"]');
  check((await alpha.getAttribute("open")) === null, "app starts folded");
  await alpha.locator("summary").click();
  await alpha.locator('input[type="range"]').waitFor();
  check(
    (await alpha.locator('input[type="range"]').inputValue()) === "0.97",
    "app inherits global threshold",
  );

  await page.locator("#t-lang").click();
  await page.locator("#p-lang .pager button").last().click();
  await page.waitForFunction(() => document.querySelectorAll(".taught li").length === 5);
  await page.locator('[data-f="learned-filter"]').fill("WORD204");
  await page.waitForFunction(() => document.querySelectorAll(".taught li").length === 1);
  check(
    (await page.locator(".taught code").textContent()) === "word204",
    "search reaches past first page",
  );
  await page.getByRole("button", { name: "word204 잊기", exact: true }).click();
  await page.getByText("검색에 맞는 학습 단어가 없습니다.", { exact: true }).waitFor();

  await page.route(
    "**/api/learning",
    (route) => route.fulfill({ status: 500, json: { error: "test learning failure" } }),
    { times: 1 },
  );
  await page.reload();
  await page.getByText(/학습 정보를 불러오지 못했습니다/).waitFor();
  await page.locator("#p-lang").getByRole("button", { name: "다시 시도", exact: true }).click();
  await page.locator('[data-f="learned-filter"]').waitFor();

  await page.locator("#chat-in").focus();
  await page.keyboard.press("Shift+Tab");
  check(
    await page.evaluate(() => document.activeElement.id !== "chat-in"),
    "Shift+Tab leaves chat",
  );
  await page.evaluate(async () => {
    chat.keys = Array(400).fill("1");
    chat.draft = await api("/api/play", { keys: chat.keys });
    drawChat();
  });
  await page.locator("#chat-in").focus();
  await page.keyboard.press("Backspace");
  await page.waitForFunction(() => document.querySelector("#chat-in").textContent.length === 399);
  await page.keyboard.press("Enter");
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => document.querySelector("#chat-in").textContent === "");
  check((await page.locator(".bubble").count()) > 0, "full draft can submit");

  let releasePlay;
  const playGate = new Promise((resolve) => {
    releasePlay = resolve;
  });
  await page.route(
    "**/api/play",
    async (route) => {
      await playGate;
      await route.continue();
    },
    { times: 1 },
  );
  const playing = page.waitForRequest("**/api/play");
  await page.keyboard.press("a");
  await playing;
  await page.evaluate(() => {
    window.pendingChatCheck = chat.queue;
  });
  await page.getByRole("button", { name: "지우기", exact: true }).click();
  releasePlay();
  await page.evaluate(() => window.pendingChatCheck);
  check((await page.locator("#chat-in").textContent()) === "", "clear discards pending input");

  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator("#test-jump").click();
  check(
    await page.evaluate(() => document.activeElement.id === "chat-in"),
    "narrow screen test jump",
  );
  check(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    "no page overflow",
  );
  check(errors.length === 0, errors.join("\n"));
  return "dashboard browser checks passed";
}
