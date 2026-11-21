// The page-side helpers of the accessibility checks (M1.6-QA-003), shared by a11y.mjs and the
// suites that audit surfaces a11y.mjs cannot reach (models.mjs: the first-run setup). Installed
// with `wd.exec(HELPERS)` as `window.__a11y`: describe, focusVisible, clipped, contrastScan,
// targets.
export const HELPERS = `
window.__a11y = (() => {
  const region = (el) => {
    if (!el || el === document.body) return "body";
    if (el.closest('[role=dialog]')) return "dialog";
    if (el.closest('.titlerow')) return "title";
    if (el.closest('[data-region=files]')) return "files";
    if (el.closest('.tabs')) return "tabs";
    if (el.closest('.monaco-editor')) return "editor";
    if (el.closest('[data-region=editor]')) return "editor-area";
    if (el.closest('[data-region=console]')) return "console";
    if (el.closest('.statusbar')) return "status";
    return "other";
  };
  const text = (el) => (el?.textContent ?? "").replace(/\\s+/g, " ").trim();
  const name = (el) => {
    if (!el) return "";
    const by = el.getAttribute("aria-labelledby");
    if (by) return by.split(/\\s+/).map((id) => text(document.getElementById(id))).join(" ").trim();
    return (el.getAttribute("aria-label") || (el.labels && el.labels[0] && text(el.labels[0])) || text(el) || el.getAttribute("title") || el.getAttribute("placeholder") || "").trim();
  };
  const IMPLICIT = { BUTTON: "button", A: "link", INPUT: "textbox", TEXTAREA: "textbox", SELECT: "combobox" };
  const role = (el) => el?.getAttribute("role") || IMPLICIT[el?.tagName] || "";
  const describe = (el) => ({ region: region(el), role: role(el), name: name(el).slice(0, 60), tag: el?.tagName?.toLowerCase(), cls: (el?.className?.baseVal ?? el?.className ?? "").toString().slice(0, 40) });

  // Focus indication: an outline ≥ 2 px, a box-shadow ring, or (rows) the focused row's ring.
  const focusVisible = (el) => {
    const cs = getComputedStyle(el);
    if (cs.outlineStyle !== "none" && parseFloat(cs.outlineWidth) >= 2) return "outline " + cs.outlineWidth;
    if (cs.boxShadow && cs.boxShadow !== "none") return "ring";
    if (el.getAttribute("role") === "tree") {
      const row = document.getElementById(el.getAttribute("aria-activedescendant") || "");
      if (row && getComputedStyle(row).boxShadow !== "none") return "row ring";
      if (row && getComputedStyle(row, "::after").content !== "none") return "row marker";
    }
    if (el.closest(".monaco-editor")) return "caret";
    // Borderless inputs inside a recessed well: the well draws the ring on focus-within.
    const well = el.closest(".prompt, .palette__bar");
    if (well && getComputedStyle(well).boxShadow.split("inset").length > 2) return "well ring";
    // The Chat composer's box draws a 2 px focus outline on focus-within (M3-UI-001).
    const box = el.closest(".composer__box");
    if (box) {
      const bs = getComputedStyle(box);
      if (bs.outlineStyle !== "none" && parseFloat(bs.outlineWidth) >= 2) return "box outline";
    }
    return "";
  };
  const clipped = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return "zero-size";
    for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
      const cs = getComputedStyle(p);
      if (!/(auto|scroll|hidden)/.test(cs.overflow + cs.overflowX + cs.overflowY)) continue;
      const pr = p.getBoundingClientRect();
      if (r.right < pr.left + 1 || r.left > pr.right - 1 || r.bottom < pr.top + 1 || r.top > pr.bottom - 1) return "outside its scroll container";
    }
    return "";
  };

  // ---- colour ----
  const parse = (c) => {
    const m = /rgba?\\(([^)]+)\\)/.exec(c || "");
    if (!m) return null;
    const p = m[1].split(/[\\s,/]+/).filter(Boolean).map(Number);
    return [p[0], p[1], p[2], p.length > 3 ? p[3] : 1];
  };
  const over = (top, bottom) => {
    const a = top[3];
    return [0, 1, 2].map((i) => top[i] * a + bottom[i] * (1 - a)).concat(1);
  };
  const lum = (c) => {
    const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
    return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]);
  };
  const ratio = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };
  // The backgrounds under an element: fills from the element up, composited; a gradient
  // contributes each of its stops (the check takes the worst).
  const backgrounds = (el) => {
    const layers = [];
    for (let p = el; p; p = p.parentElement) {
      const cs = getComputedStyle(p);
      const stops = (cs.backgroundImage.match(/rgba?\\([^)]+\\)/g) || []).map(parse).filter(Boolean);
      const fill = parse(cs.backgroundColor);
      layers.push({ fill: fill && fill[3] > 0 ? fill : null, stops });
      if ((fill && fill[3] >= 1) || stops.some((s) => s[3] >= 1)) break;
    }
    let bases = [[255, 255, 255, 1]];
    for (const l of layers.reverse()) {
      if (l.stops.length) bases = bases.flatMap((b) => l.stops.map((s) => over(s, b)));
      if (l.fill) bases = bases.map((b) => over(l.fill, b));
    }
    return bases;
  };
  const opacity = (el) => { let o = 1; for (let p = el; p; p = p.parentElement) o *= parseFloat(getComputedStyle(p).opacity); return o; };
  const visuallyHidden = (el) => { const r = el.getBoundingClientRect(); return r.width <= 1 || r.height <= 1; };
  const faint = () => { const s = document.createElement("span"); s.style.color = "var(--text-faint)"; document.body.appendChild(s); const c = getComputedStyle(s).color; s.remove(); return c; };

  const contrastScan = () => {
    const FAINT = faint();
    const out = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const seen = new Set();
    let n;
    while ((n = walker.nextNode())) {
      if (!n.nodeValue.trim()) continue;
      const el = n.parentElement;
      if (!el || seen.has(el)) continue;
      seen.add(el);
      if (el.closest("[aria-hidden=true], [hidden], .visually-hidden, script, style, .monaco-editor .view-overlays, .minimap")) continue;
      if (el.closest("[disabled], [aria-disabled=true], :disabled")) continue; // inactive controls are exempt (WCAG 1.4.3)
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0 || r.bottom < 0 || r.top > innerHeight || visuallyHidden(el)) continue;
      const cs = getComputedStyle(el);
      if (cs.visibility === "hidden" || parseFloat(cs.opacity) === 0) continue;
      const fg = parse(cs.color);
      if (!fg) continue;
      const o = opacity(el);
      const nonessential = cs.color === FAINT || el.closest(".line-numbers") !== null;
      const large = parseFloat(cs.fontSize) >= 24 || (parseFloat(cs.fontSize) >= 18.66 && parseInt(cs.fontWeight) >= 700);
      const need = nonessential || large ? 3 : 4.5;
      let worst = Infinity;
      for (const bg of backgrounds(el)) worst = Math.min(worst, ratio(over([fg[0], fg[1], fg[2], fg[3] * o], bg), bg));
      if (worst < need) out.push({ text: n.nodeValue.trim().slice(0, 40), ratio: +worst.toFixed(2), need, region: region(el), cls: (el.className?.baseVal ?? el.className ?? "").toString().slice(0, 40), color: cs.color });
    }
    return out;
  };

  const targets = () => {
    const out = [];
    for (const el of document.querySelectorAll("button, [role=button], [role=tab], [role=menuitem], [role=switch], [role=radio], input, a[href], [role=treeitem]")) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0 || el.closest("[aria-hidden=true], .monaco-editor")) continue;
      if (r.width < 24 || r.height < 24) out.push({ ...describe(el), w: Math.round(r.width), h: Math.round(r.height) });
    }
    return out;
  };
  return { region, describe, focusVisible, clipped, contrastScan, targets };
})();
true;`;
