//! Single-file HTML rendering for the playground.

use crate::{Case, Feature};

pub struct Document {
    pub features: Vec<Feature>,
    pub cases: Vec<Case>,
    pub evidence_generated_at: String,
    pub evidence_targets: serde_json::Value,
    pub evidence_git: serde_json::Value,
    pub stale: bool,
}

pub fn render(document: &Document) -> String {
    let features: Vec<serde_json::Value> = document
        .features
        .iter()
        .map(|feature| {
            serde_json::json!({
                "id": feature.id,
                "title": feature.title,
                "spec": feature.spec,
            })
        })
        .collect();

    let cases: Vec<serde_json::Value> = document
        .cases
        .iter()
        .map(|case| {
            serde_json::json!({
                "id": case.id,
                "title": case.title,
                "why": case.why,
                "feature": case.feature,
                "kind": case.kind,
                "source": case.source,
                "sourceHtml": case.source_html,
                "outputs": case.outputs,
                "diagnostic": case.diagnostic,
                "status": case.status,
            })
        })
        .collect();

    let data = serde_json::json!({
        "features": features,
        "cases": cases,
        "evidenceGeneratedAt": document.evidence_generated_at,
        "evidenceTargets": document.evidence_targets,
        "evidenceGit": document.evidence_git,
        "stale": document.stale,
    });

    TEMPLATE.replace("__DATA__", &data.to_string())
}

const TEMPLATE: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Lingua Tungga playground</title>
<style>
:root {
  color-scheme: light dark;
  --bg: #f7f7f5;
  --panel: #ffffff;
  --ink: #1d1d1f;
  --muted: #6b6b70;
  --line: #dcdcd6;
  --accent: #3b5bdb;
  --ok: #2f9e44;
  --bad: #c92a2a;
  --warn: #b8860b;
  --code: #f1f1ee;
  --kw: #7048e8;
  --str: #0b7285;
  --num: #c2410c;
  --type: #1c7ed6;
  --comment: #868e96;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #16161a;
    --panel: #1e1e24;
    --ink: #ececef;
    --muted: #9a9aa2;
    --line: #2e2e36;
    --accent: #8da2ff;
    --ok: #69db7c;
    --bad: #ff8787;
    --warn: #ffd43b;
    --code: #141418;
    --kw: #b197fc;
    --str: #66d9e8;
    --num: #ffa94d;
    --type: #74c0fc;
    --comment: #7a7a85;
  }
}
* { box-sizing: border-box; }
body { margin: 0; font: 15px/1.5 system-ui, -apple-system, Segoe UI, sans-serif; background: var(--bg); color: var(--ink); }
header { padding: 16px 24px; border-bottom: 1px solid var(--line); background: var(--panel); position: sticky; top: 0; z-index: 10; }
header h1 { font-size: 18px; margin: 0 0 4px; }
header .meta { color: var(--muted); font-size: 13px; }
.banner { margin-top: 8px; padding: 8px 12px; border-radius: 6px; font-size: 13px; }
.banner.stale { background: color-mix(in srgb, var(--warn) 18%, transparent); border: 1px solid var(--warn); }
.layout { display: grid; grid-template-columns: 320px 1fr; min-height: calc(100vh - 84px); }
aside { border-right: 1px solid var(--line); padding: 12px; overflow: auto; height: calc(100vh - 84px); position: sticky; top: 84px; }
#search { width: 100%; padding: 8px 10px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--ink); margin-bottom: 10px; }
details { margin: 2px 0; }
summary { cursor: pointer; padding: 4px 6px; border-radius: 4px; font-weight: 600; font-size: 13px; color: var(--muted); }
summary:hover { background: var(--code); }
.case-link { display: block; padding: 5px 10px; border-radius: 4px; color: var(--ink); text-decoration: none; font-size: 13px; }
.case-link:hover { background: var(--code); }
.case-link.active { background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); }
main { padding: 24px; max-width: 1100px; }
h2 { margin: 0 0 6px; font-size: 22px; }
.why { color: var(--muted); margin: 0 0 14px; max-width: 80ch; }
.badges { display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 16px; }
.badge { font-size: 12px; padding: 3px 8px; border-radius: 999px; border: 1px solid var(--line); color: var(--muted); }
.badge.pass { color: var(--ok); border-color: var(--ok); }
.badge.fail { color: var(--bad); border-color: var(--bad); }
.badge.not-run { color: var(--warn); border-color: var(--warn); }
.feature-tag { font-size: 12px; color: var(--muted); margin-bottom: 12px; }
.block { margin-bottom: 18px; }
.block-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px; }
.block-title { font-size: 13px; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: .04em; }
button.copy { font-size: 12px; padding: 3px 10px; border-radius: 6px; border: 1px solid var(--line); background: var(--panel); color: var(--muted); cursor: pointer; }
button.copy:hover { border-color: var(--accent); color: var(--accent); }
pre { margin: 0; padding: 14px 16px; background: var(--code); border: 1px solid var(--line); border-radius: 8px; overflow: auto; font: 13px/1.55 ui-monospace, SFMono-Regular, Menlo, monospace; }
.tok-kw { color: var(--kw); }
.tok-str { color: var(--str); }
.tok-num { color: var(--num); }
.tok-type { color: var(--type); }
.tok-comment { color: var(--comment); font-style: italic; }
.tabs { display: flex; gap: 4px; margin-bottom: 6px; }
.tabs button { font-size: 13px; padding: 5px 12px; border-radius: 6px 6px 0 0; border: 1px solid var(--line); border-bottom: none; background: var(--panel); color: var(--muted); cursor: pointer; }
.tabs button.active { color: var(--accent); border-color: var(--accent); }
.empty { color: var(--muted); font-style: italic; }
.diag { border-left: 3px solid var(--bad); padding: 8px 12px; background: var(--code); border-radius: 6px; font-family: ui-monospace, Menlo, monospace; font-size: 13px; white-space: pre-wrap; }
.diag.ok { border-color: var(--ok); }
@media (max-width: 820px) {
  .layout { grid-template-columns: 1fr; }
  aside { position: static; height: auto; border-right: none; border-bottom: 1px solid var(--line); }
}
</style>
</head>
<body>
<header>
  <h1>Lingua Tungga playground</h1>
  <div class="meta" id="meta"></div>
  <div id="stale"></div>
</header>
<div class="layout">
  <aside>
    <input id="search" type="search" placeholder="Cari kasus, fitur, atau source… (tekan /)" autocomplete="off">
    <nav id="nav"></nav>
  </aside>
  <main id="main"></main>
</div>
<script>
const DATA = __DATA__;
const cases = new Map(DATA.cases.map(c => [c.id, c]));
const featureById = new Map(DATA.features.map(f => [f.id, f]));
const TARGETS = ["ts", "py", "go"];

const nav = document.getElementById("nav");
const main = document.getElementById("main");
const search = document.getElementById("search");
const meta = document.getElementById("meta");

const runCases = DATA.cases.filter(c => c.kind === "run").length;
const diagCases = DATA.cases.filter(c => c.kind === "diagnostic").length;
let evidence = DATA.evidenceGeneratedAt ? ` · evidence ${DATA.evidenceGeneratedAt}` : " · evidence belum ada";
if (DATA.evidenceGit && DATA.evidenceGit.commit) evidence += ` @ ${DATA.evidenceGit.commit}${DATA.evidenceGit.dirty ? " (dirty)" : ""}`;
meta.textContent = `${runCases} kasus executable · ${diagCases} kasus diagnostik · ${DATA.features.length} fitur` + evidence;
const t = DATA.evidenceTargets || {};
const summaries = ["ts", "py", "go"].filter(k => t[k]).map(k => `${k}: ${t[k].passed}/${t[k].cases}` + (t[k].conformance ? ` (+${t[k].conformance_passed}/${t[k].conformance} conformance)` : ""));
if (summaries.length) meta.textContent += " · " + summaries.join(" · ");
if (DATA.stale) {
  const el = document.getElementById("stale");
  el.className = "banner stale";
  el.textContent = "evidence.json tidak cocok dengan daftar kasus; jalankan scripts/playground.sh agar status terisi.";
}

function badge(status) {
  const label = status === "pass" ? "lulus" : status === "fail" ? "gagal" : status === "ok" ? "ok" : "belum";
  const cls = status === "pass" || status === "ok" ? "pass" : status === "fail" ? "fail" : "not-run";
  return `<span class="badge ${cls}">${label}</span>`;
}

function statusBadges(c) {
  if (c.kind === "diagnostic") return `<span class="badge pass">diagnostik ok</span>`;
  return TARGETS.map(t => `<span class="badge ${c.status[t] === "pass" ? "pass" : c.status[t] === "fail" ? "fail" : "not-run"}">${t}: ${c.status[t] === "pass" ? "lulus" : c.status[t] === "fail" ? "gagal" : "belum"}</span>`).join(" ");
}

function renderNav(filter = "") {
  const needle = filter.trim().toLowerCase();
  const groups = new Map(DATA.features.map(f => [f.id, []]));
  for (const c of DATA.cases) {
    const haystack = (c.id + " " + c.title + " " + c.feature + " " + c.source).toLowerCase();
    if (needle && !haystack.includes(needle)) continue;
    groups.get(c.feature)?.push(c);
  }
  nav.innerHTML = "";
  for (const feature of DATA.features) {
    const items = groups.get(feature.id);
    if (!items || items.length === 0) continue;
    const details = document.createElement("details");
    details.open = true;
    const summary = document.createElement("summary");
    summary.textContent = feature.title;
    details.appendChild(summary);
    for (const c of items) {
      const link = document.createElement("a");
      link.href = "#/" + c.id;
      link.className = "case-link" + (location.hash === "#/" + c.id ? " active" : "");
      link.textContent = c.title;
      link.onclick = () => setTimeout(() => markActive(c.id), 0);
      details.appendChild(link);
    }
    nav.appendChild(details);
  }
}

function markActive(id) {
  for (const link of nav.querySelectorAll(".case-link")) {
    link.classList.toggle("active", link.getAttribute("href") === "#/" + id);
  }
}

function copyButton(text) {
  const button = document.createElement("button");
  button.className = "copy";
  button.textContent = "copy";
  button.onclick = async () => {
    try { await navigator.clipboard.writeText(text); button.textContent = "copied"; }
    catch { button.textContent = "select manually"; }
    setTimeout(() => { button.textContent = "copy"; }, 1200);
  };
  return button;
}

function block(title, html, raw) {
  const wrapper = document.createElement("div");
  wrapper.className = "block";
  const head = document.createElement("div");
  head.className = "block-head";
  const label = document.createElement("div");
  label.className = "block-title";
  label.textContent = title;
  head.appendChild(label);
  if (raw !== undefined) head.appendChild(copyButton(raw));
  wrapper.appendChild(head);
  const pre = document.createElement("pre");
  pre.innerHTML = html;
  wrapper.appendChild(pre);
  return wrapper;
}

function esc(text) {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function renderCase(id) {
  const c = cases.get(id);
  if (!c) {
    main.innerHTML = `<h2>Kasus tidak ditemukan</h2><p class="why">Pilih kasus di sidebar.</p>`;
    return;
  }
  markActive(id);
  main.innerHTML = "";
  const h2 = document.createElement("h2");
  h2.textContent = c.title;
  main.appendChild(h2);
  const feature = featureById.get(c.feature);
  const tag = document.createElement("div");
  tag.className = "feature-tag";
  tag.textContent = feature ? `fitur: ${feature.title}${feature.spec ? " · " + feature.spec : ""}` : c.feature;
  main.appendChild(tag);
  if (c.why) {
    const why = document.createElement("p");
    why.className = "why";
    why.textContent = c.why;
    main.appendChild(why);
  }
  const badges = document.createElement("div");
  badges.className = "badges";
  badges.innerHTML = statusBadges(c) + `<span class="badge">${c.kind === "diagnostic" ? "diagnostik" : "executable"}</span>`;
  main.appendChild(badges);

  main.appendChild(block("source (.lt)", c.sourceHtml, c.source));

  if (c.kind === "diagnostic") {
    const diag = document.createElement("div");
    diag.className = "diag ok";
    diag.textContent = c.diagnostic || "";
    main.appendChild(block("diagnostic (wajib gagal)"));
    main.lastChild.appendChild(diag);
    return;
  }

  const tabs = document.createElement("div");
  tabs.className = "tabs";
  const panes = document.createElement("div");
  let current = TARGETS[0];
  const draw = () => {
    tabs.innerHTML = "";
    for (const t of TARGETS) {
      const b = document.createElement("button");
      b.textContent = t === "ts" ? "TypeScript" : t === "py" ? "Python" : "Go";
      b.className = t === current ? "active" : "";
      b.onclick = () => { current = t; draw(); };
      tabs.appendChild(b);
    }
    panes.innerHTML = "";
    const code = c.outputs[current] || "";
    panes.appendChild(block("output " + current, `<code>${esc(code)}</code>`, code));
  };
  draw();
  main.appendChild(tabs);
  main.appendChild(panes);
}

function route() {
  const id = decodeURIComponent(location.hash.replace(/^#\//, ""));
  if (id) renderCase(id);
  else renderCase(DATA.cases[0]?.id);
}

search.addEventListener("input", () => renderNav(search.value));
window.addEventListener("hashchange", route);
document.addEventListener("keydown", (event) => {
  if (event.key === "/" && document.activeElement !== search) { event.preventDefault(); search.focus(); }
});
renderNav();
route();
</script>
</body>
</html>
"##;
