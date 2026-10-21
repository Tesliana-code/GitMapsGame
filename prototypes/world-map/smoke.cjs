// Interaction smoke tests for the world-map prototype previews (jsdom; CSP is checked separately
// in a real browser by browser_check.sh). Expected values are computed here from each preview's
// map.json, independently of the page's own logic.
//
//   npm install --prefix /tmp/gm-jsdom jsdom@24.1.3
//   NODE_PATH=/tmp/gm-jsdom/node_modules node prototypes/world-map/smoke.cjs
"use strict";
const { JSDOM, VirtualConsole } = require("jsdom");
const fs = require("fs");
const path = require("path");

const SITE = path.resolve(__dirname, "../../site/prototype");
let passed = 0, failed = 0;
function check(name, ok, detail) {
  if (ok) { passed++; } else { failed++; console.log("FAIL " + name + (detail ? " :: " + detail : "")); }
}
function load(preview, hash) {
  const file = path.join(SITE, preview, "index.html");
  const errors = [];
  const vc = new VirtualConsole();
  vc.on("jsdomError", e => errors.push(String(e && e.message || e)));
  const dom = new JSDOM(fs.readFileSync(file, "utf8"), {
    runScripts: "dangerously", url: "file://" + file + (hash || ""), virtualConsole: vc, pretendToBeVisual: true,
  });
  return { dom, d: dom.window.document, w: dom.window, errors };
}
function map(preview) { return JSON.parse(fs.readFileSync(path.join(SITE, preview, "map.json"), "utf8")); }
function click(w, el) { el.dispatchEvent(new w.MouseEvent("click", { bubbles: true })); }
function text(el) { return el ? el.textContent.replace(/\s+/g, " ").trim() : ""; }
function expectedStart(m) {
  const by = s => m.crossings.filter(c => c.state === s).sort((a, b) => a.number - b.number)[0];
  if (!m.crossings.length) return "setup";
  for (const s of ["DISCOVERED", "CLAIMED"]) if (by(s)) return by(s).number;
  if (m.crossings.every(c => c.state === "PROVEN")) return "conquered";
  for (const s of ["INVALID", "CLOSED_UNPROVEN", "LOCKED"]) if (by(s)) return by(s).number;
}
const WORDS = { DISCOVERED: "DISCOVERED", CLAIMED: "CLAIMED", PROVEN: "PROVEN", LOCKED: "LOCKED", CLOSED_UNPROVEN: "CLOSED, UNPROVEN", INVALID: "INVALID" };

function common(name, t, m) {
  const { d } = t;
  check(name + ": no script errors", t.errors.length === 0, t.errors.join(" | "));
  check(name + ": pixel lettering present with screen-reader text", d.querySelector("#orders-h svg.px") && text(d.querySelector("#orders-h .sr-only")) === "ORDERS");
  check(name + ": START GAME and TRACK YOUR PROGRESS are buttons with accessible names",
    d.querySelector("#start").tagName === "BUTTON" && text(d.querySelector("#start .sr-only")) === "START GAME" && text(d.querySelector("#track .sr-only")) === "TRACK YOUR PROGRESS");
  check(name + ": legend shows all six states with art and words", d.querySelectorAll("#legend li").length === 6 && [...d.querySelectorAll("#legend li")].every(li => li.querySelector("svg.art")));
  check(name + ": evidence and rules collapsed by default", !d.querySelector("#evidence").open && !d.querySelector("#rules").open);
  check(name + ": reduced-motion rule present", /prefers-reduced-motion:reduce/.test(d.querySelector("style").textContent));
  check(name + ": no javascript: links or inline handlers", ![...d.querySelectorAll("a")].some(a => /^\s*javascript:/i.test(a.getAttribute("href") || "")) && !d.querySelector("[onclick],[onerror],[onload]"));
  check(name + ": every button has an accessible name", [...d.querySelectorAll("button")].every(b => (b.getAttribute("aria-label") || text(b)).length > 0));
  if (m) {
    const tiles = [...d.querySelectorAll("#grid .tile")];
    check(name + ": one tile per crossing", tiles.length === m.crossings.length, tiles.length + " vs " + m.crossings.length);
    check(name + ": every tile states its state in words, glyph text and art", m.crossings.every(c => {
      const t = d.getElementById("tile-" + c.number);
      return t && t.getAttribute("aria-label").includes("State: " + WORDS[c.state]) && text(t.querySelector(".st")).includes(WORDS[c.state]) && t.querySelector("svg.art");
    }));
    click(t.w, d.getElementById("view-list"));
    check(name + ": list view lists every crossing", d.querySelectorAll("#list li").length === Math.max(m.crossings.length, 1) && !d.getElementById("list").hidden);
    click(t.w, d.getElementById("view-map"));
  }
}

// real-populated
{
  const name = "real-populated", m = map(name), t = load(name), { d, w } = t;
  common(name, t, m);
  check(name + ": one footnote disclaimer, nothing in the hero", text(d.getElementById("foot")) === "LIVE REPOSITORY DATA · PLAY THROUGH GITHUB" && d.getElementById("badge").hidden && d.getElementById("tutorial-banner").hidden);
  check(name + ": status bar is pure game state", JSON.stringify([...d.querySelectorAll("#statusbar span")].map(text)) === JSON.stringify(["WORLD " + m.repository.name_with_owner, "LAST MOVE " + m.as_of, "CHARTED " + m.crossings.length, "PROVEN " + m.counts.PROVEN + "/" + m.crossings.length]));
  click(w, d.getElementById("start"));
  const exp = expectedStart(m), orders = text(d.getElementById("orders-body"));
  check(name + ": START GAME selects the expected crossing", d.getElementById("tile-" + exp).getAttribute("aria-current") === "true", "expected #" + exp);
  check(name + ": orders explain the real GitHub claim path", orders.includes("Closes #" + exp) && orders.includes("triage or write access") && orders.includes("ask on the issue"));
  check(name + ": orders never tell players to assign themselves", !/assign yourself/i.test(orders));
  check(name + ": orders do not claim instant automatic updates", orders.includes("next refresh") && !/map then shows|map settles it/.test(orders));
  const hrefs = [...d.querySelectorAll("#orders-body a")].map(a => a.href);
  const c = m.crossings.find(x => x.number === exp);
  check(name + ": orders link the real issue and the repository's compare page", hrefs.includes(c.url) && hrefs.includes("https://github.com/" + m.repository.name_with_owner + "/compare"));
  check(name + ": focus moves to ORDERS", d.activeElement && d.activeElement.id === "orders-h");
  click(w, d.getElementById("track"));
  check(name + ": TRACK focuses the LEDGER", d.activeElement && d.activeElement.id === "ledger-h");
  const roles = login => m.crossings.filter(c => c.assignees.includes(login) || c.pull_requests.some(p => p.author === login || p.review.approved_by.concat(p.review.changes_requested_by).includes(login)));
  for (const login of ["babakks", "heaths"]) {
    const input = d.getElementById("player-login"); input.value = login; click(w, d.getElementById("player-go"));
    const got = [...d.querySelectorAll("#player-results li")].map(li => text(li));
    const want = roles(login).map(c => c.number);
    check(name + ": ledger finds @" + login + " in exactly the right crossings", got.length === want.length && want.every(n => got.some(g => g.startsWith("#" + n + " "))), JSON.stringify(got));
  }
  { const input = d.getElementById("player-login"); input.value = "nobody-in-this-snapshot"; click(w, d.getElementById("player-go"));
    check(name + ": unknown login reports no moves (no guessing)", text(d.getElementById("ledger-body")).includes("No moves by @nobody-in-this-snapshot in this snapshot.")); }
  { const input = d.getElementById("player-login"); input.value = "not a login!"; click(w, d.getElementById("player-go"));
    check(name + ": invalid login rejected", text(d.getElementById("ledger-body")).includes("not a valid GitHub login")); }
  { d.getElementById("evidence").open = true; const ev = text(d.getElementById("evidence-body"));
    check(name + ": evidence keeps provenance and export files", ev.includes(m.source.snapshot_sha256) && ["map.json", "snapshot.json", "SHA256SUMS"].every(f => [...d.querySelectorAll("#evidence-body a")].some(a => a.getAttribute("href") === f))); }
  const deep = load(name, "#player=babakks");
  check(name + ": #player= deep link fills the ledger", text(deep.d.getElementById("ledger-body")).includes("@babakks appears in"));
  const sel = load(name, "#crossing=3718");
  check(name + ": #crossing= deep link selects the crossing", sel.d.getElementById("tile-3718").getAttribute("aria-current") === "true");
}

// real-empty
{
  const name = "real-empty", m = map(name), t = load(name), { d, w } = t;
  common(name, t, m);
  check(name + ": uncharted map, no invented tiles", d.querySelectorAll("#grid .tile").length === 0 && text(d.getElementById("grid")).includes("NO CROSSINGS DEFINED YET"));
  click(w, d.getElementById("start"));
  const quest = text(d.getElementById("quest-body"));
  check(name + ": START GAME opens the SETUP QUEST", !d.getElementById("quest").hidden && d.activeElement && d.activeElement.id === "quest-h");
  check(name + ": quest states the world is not initialized and the permissions needed", quest.includes("WORLD NOT INITIALIZED") && quest.includes("write access") && quest.includes("triage or write access"));
  click(w, d.getElementById("track"));
  check(name + ": ledger shows zero charted", text(d.getElementById("ledger-body")).includes("0 CROSSINGS CHARTED."));
  click(w, d.getElementById("enter-tutorial"));
  check(name + ": tutorial switch is labelled fictional by chip and footnote", text(d.getElementById("badge")) === "TUTORIAL WORLD · FICTIONAL" && text(d.getElementById("foot")) === "TUTORIAL WORLD · FICTIONAL · NO GITHUB CHANGES" && d.getElementById("tutorial-banner").hidden && d.querySelectorAll("#grid .tile").length === 4);
}

// tutorial
{
  const name = "tutorial", t = load(name), { d, w } = t;
  common(name, t, null);
  check(name + ": fictional labels: chip, status bar and one footnote, no warning panel", text(d.getElementById("badge")) === "TUTORIAL WORLD · FICTIONAL" && text(d.getElementById("statusbar")).includes("TUTORIAL (FICTIONAL)") && text(d.getElementById("foot")) === "TUTORIAL WORLD · FICTIONAL · NO GITHUB CHANGES" && d.getElementById("tutorial-banner").hidden);
  const btn = label => [...d.querySelectorAll("#orders-body button")].find(b => text(b) === label);
  for (let n = 1; n <= 4; n++) {
    click(w, d.getElementById("start"));
    check(name + ": START GAME selects #" + n, d.getElementById("tile-" + n).getAttribute("aria-current") === "true");
    click(w, btn("CLAIM (TUTORIAL)"));
    check(name + ": #" + n + " becomes CLAIMED", text(d.querySelector("#tile-" + n + " .st")).includes("CLAIMED"));
    click(w, btn("SUBMIT PROOF (TUTORIAL)"));
    check(name + ": #" + n + " becomes PROVEN" + (n < 4 ? " and unlocks #" + (n + 1) : ""), text(d.querySelector("#tile-" + n + " .st")).includes("PROVEN") &&
      (n === 4 || text(d.querySelector("#tile-" + (n + 1) + " .st")).includes("DISCOVERED")));
  }
  click(w, d.getElementById("start"));
  check(name + ": START GAME after victory reports WORLD CONQUERED", text(d.getElementById("orders-body")).includes("WORLD CONQUERED"));
  const gate = label => [...d.querySelectorAll("#mock-body button")].find(b => text(b) === label);
  click(w, gate("RUN MOCK CI")); click(w, gate("GRANT MOCK APPROVAL")); click(w, gate("ATTEMPT MOCK RELEASE"));
  check(name + ": mock release needs CI and approval, and says nothing was deployed", text(d.getElementById("mock-body")).includes("MOCK RELEASE VERIFIED. No real deployment happened."));
}

// constructed-stress
{
  const name = "constructed-stress", m = map(name), t = load(name), { d, w } = t;
  common(name, t, m);
  check(name + ": identified by one footnote disclaimer", text(d.getElementById("foot")) === "TEST WORLD · CONSTRUCTED DATA · LOCAL PREVIEW" && d.getElementById("badge").hidden);
  check(name + ": no overlay, warning box or hero chip", !d.getElementById("watermark") && !d.getElementById("constructed-banner") && !d.querySelector(".warn:not([hidden])"));
  check(name + ": never claims GitHub provenance", !/GITHUB/.test(text(d.getElementById("statusbar"))) && !/GITHUB/.test(text(d.getElementById("foot"))));
  check(name + ": footnote is 9-10px", /footer\{[^}]*font-size:(9|10)px/.test(d.querySelector("style").textContent));
  d.getElementById("evidence").open = true;
  check(name + ": evidence drawer explains the data truthfully", text(d.getElementById("evidence-body")).includes("not from a live repository"));
  check(name + ": no clickable external links", [...d.querySelectorAll("a")].every(a => !/^https?:/i.test(a.getAttribute("href") || "")));
  check(name + ": hostile title stays text", d.querySelectorAll("img, #grid b").length === 0 && text(d.getElementById("tile-33")).includes("</script><b>hostile title</b>"));
  check(name + ": cycle and missing dependency sit in BROKEN LANDS", [...d.querySelectorAll(".colhead")].some(h => text(h).startsWith("BROKEN LANDS")));
  const heads = [...d.querySelectorAll(".colhead")].map(h => ({ left: parseInt(h.style.left, 10), label: text(h) }));
  check(name + ": wrapped columns continue the same stage", heads.some(h => h.label === "(CONTINUED)") && heads[0].label === "STARTING LANDS");
  const columnOf = n => { const left = parseInt(d.getElementById("tile-" + n).style.left, 10) - 2; let label = null;
    for (const h of heads) { if (h.left <= left) label = h.label === "(CONTINUED)" ? label : h.label; } return label; };
  check(name + ": a crossing that requires a broken one sits beyond the starting lands", columnOf(32) !== "STARTING LANDS" && columnOf(32) !== null, String(columnOf(32)));
  click(w, d.getElementById("start"));
  check(name + ": START GAME selects the lowest DISCOVERED crossing", d.getElementById("tile-" + expectedStart(m)).getAttribute("aria-current") === "true");
  // Real fixtures have no assignees; exercise every ledger role on the constructed world.
  const roles = login => m.crossings.filter(c => c.assignees.includes(login) || c.pull_requests.some(p => p.author === login || p.review.approved_by.concat(p.review.changes_requested_by).includes(login)));
  click(w, d.getElementById("track"));
  for (const login of ["test-player-1", "test-player-4", "test-player-6"]) {
    const input = d.getElementById("player-login"); input.value = login; click(w, d.getElementById("player-go"));
    const got = [...d.querySelectorAll("#player-results li")].map(li => text(li)), want = roles(login).map(c => c.number);
    check(name + ": ledger finds @" + login + " in exactly the right crossings", want.length > 0 && got.length === want.length && want.every(n => got.some(g => g.startsWith("#" + n + " "))), JSON.stringify(got));
  }
  check(name + ": an assignee-only player is found", roles("test-player-4").some(c => c.assignees.includes("test-player-4")));
}

console.log(passed + " passed, " + failed + " failed");
process.exit(failed ? 1 : 0);
