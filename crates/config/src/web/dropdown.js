// A dropdown that looks like the rest of the page (listbox pattern).
//   dd({ value, options: [[value, text], ...], label, onchange, key, disabled, title })
// `.value` of the result is the chosen option's value; `onchange(value)` runs only when another
// option is chosen. Keys: Up Down Home End, Enter/Space to choose, Esc to close.

let openDd = null; // the one dropdown that is open
let ddCount = 0;

function closeDd(refocus) {
  if (!openDd) return;
  openDd.menu.hidden = true;
  openDd.btn.setAttribute("aria-expanded", "false");
  openDd.btn.removeAttribute("aria-activedescendant");
  if (refocus) openDd.btn.focus();
  openDd = null;
}
document.addEventListener("pointerdown", (e) => {
  if (openDd && !openDd.root.contains(e.target)) closeDd(false);
});

function dd({ value, options, label, onchange, key, disabled, title }) {
  const id = "dd" + ++ddCount;
  let cur = Math.max(
    0,
    options.findIndex((o) => String(o[0]) === String(value)),
  );
  let active = cur;

  const shown = h("span", {}, options[cur][1]);
  const btn = h(
    "button",
    {
      type: "button",
      class: "dd-btn",
      "aria-haspopup": "listbox",
      "aria-expanded": "false",
      "aria-label": label + ": " + options[cur][1],
      "data-f": key,
      disabled,
      title,
    },
    shown,
    h("i", { class: "dd-caret", "aria-hidden": "true" }),
  );
  const items = options.map((o, i) =>
    h(
      "li",
      {
        role: "option",
        id: id + "-" + i,
        class: "dd-opt",
        "aria-selected": i === cur,
        onpointermove: () => mark(i),
        onclick: () => choose(i),
      },
      o[1],
    ),
  );
  const menu = h(
    "ul",
    { class: "dd-menu", role: "listbox", "aria-label": label, hidden: true },
    items,
  );
  const root = h("div", { class: "dd" }, btn, menu);
  Object.defineProperty(root, "value", { get: () => options[cur][0] });
  const self = { root, btn, menu };

  function mark(i) {
    active = Math.max(0, Math.min(options.length - 1, i));
    items.forEach((el, k) => el.classList.toggle("active", k === active));
    btn.setAttribute("aria-activedescendant", id + "-" + active);
    items[active].scrollIntoView({ block: "nearest" });
  }

  function open() {
    closeDd(false);
    menu.hidden = false;
    menu.classList.remove("up", "right");
    const r = btn.getBoundingClientRect();
    if (r.left + menu.offsetWidth > document.querySelector("main").getBoundingClientRect().right)
      menu.classList.add("right");
    if (r.bottom + menu.offsetHeight + 12 > innerHeight && r.top > menu.offsetHeight + 12)
      menu.classList.add("up");
    btn.setAttribute("aria-expanded", "true");
    openDd = self;
    mark(cur);
  }

  function choose(i) {
    const changed = i !== cur;
    if (changed) {
      cur = i;
      shown.textContent = options[i][1];
      items.forEach((el, k) => el.setAttribute("aria-selected", k === i));
      btn.setAttribute("aria-label", label + ": " + options[i][1]);
    }
    closeDd(true);
    if (changed && onchange) onchange(options[i][0]);
  }

  btn.addEventListener("click", () => (openDd === self ? closeDd(false) : open()));
  btn.addEventListener("keydown", (e) => {
    const isOpen = openDd === self;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!isOpen) open();
      else mark(active + (e.key === "ArrowDown" ? 1 : -1));
    } else if (e.key === "Home" && isOpen) {
      e.preventDefault();
      mark(0);
    } else if (e.key === "End" && isOpen) {
      e.preventDefault();
      mark(options.length - 1);
    } else if ((e.key === "Enter" || e.key === " ") && isOpen) {
      e.preventDefault();
      choose(active);
    } else if (e.key === "Escape" && isOpen) {
      e.preventDefault();
      e.stopPropagation();
      closeDd(true);
    } else if (e.key === "Tab" && isOpen) closeDd(false);
  });
  return root;
}
