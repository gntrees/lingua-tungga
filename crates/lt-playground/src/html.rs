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
                "usage": case.usage,
                "usageOutput": case.usage_output,
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
pre.stdout { border-color: var(--ok); }
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
    <input id="search" type="search" placeholder="Search cases, features, or source… (press /)" autocomplete="off">
    <nav id="nav"></nav>
  </aside>
  <main id="main"></main>
</div>
<script>
const DATA = __DATA__;
const cases = new Map(DATA.cases.map(c => [c.id, c]));
const featureById = new Map(DATA.features.map(f => [f.id, f]));
const TARGETS = ["ts", "py", "go"];
const TARGET_LABEL = { ts: "TypeScript", py: "Python", go: "Go" };

const nav = document.getElementById("nav");
const main = document.getElementById("main");
const search = document.getElementById("search");
const meta = document.getElementById("meta");

const runCases = DATA.cases.filter(c => c.kind === "run").length;
const diagCases = DATA.cases.filter(c => c.kind === "diagnostic").length;
let evidence = DATA.evidenceGeneratedAt ? ` · evidence ${DATA.evidenceGeneratedAt}` : " · no evidence yet";
if (DATA.evidenceGit && DATA.evidenceGit.commit) evidence += ` @ ${DATA.evidenceGit.commit}${DATA.evidenceGit.dirty ? " (dirty)" : ""}`;
meta.textContent = `${runCases} executable cases · ${diagCases} diagnostic cases · ${DATA.features.length} features` + evidence;
const t = DATA.evidenceTargets || {};
const summaries = ["ts", "py", "go"].filter(k => t[k]).map(k => `${k}: ${t[k].passed}/${t[k].cases}` + (t[k].conformance ? ` (+${t[k].conformance_passed}/${t[k].conformance} conformance)` : ""));
if (summaries.length) meta.textContent += " · " + summaries.join(" · ");
if (DATA.stale) {
  const el = document.getElementById("stale");
  el.className = "banner stale";
  el.textContent = "evidence.json does not match the current cases; run scripts/playground.sh to fill in the status and usage output.";
}

function badge(status) {
  const label = status === "pass" ? "pass" : status === "fail" ? "fail" : status === "ok" ? "ok" : "not run";
  const cls = status === "pass" || status === "ok" ? "pass" : status === "fail" ? "fail" : "not-run";
  return `<span class="badge ${cls}">${label}</span>`;
}

function statusBadges(c) {
  if (c.kind === "diagnostic") return `<span class="badge pass">diagnostic ok</span>`;
  return TARGETS.map(t => `<span class="badge ${c.status[t] === "pass" ? "pass" : c.status[t] === "fail" ? "fail" : "not-run"}">${t}: ${c.status[t] === "pass" ? "pass" : c.status[t] === "fail" ? "fail" : "not run"}</span>`).join(" ");
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

const CODE_KEYWORDS = {
  ts: new Set(["abstract", "as", "async", "await", "break", "case", "catch", "class", "const", "constructor", "continue", "declare", "default", "delete", "do", "else", "enum", "export", "extends", "false", "finally", "for", "from", "function", "get", "if", "implements", "import", "in", "instanceof", "interface", "keyof", "let", "new", "null", "of", "private", "protected", "public", "readonly", "return", "set", "static", "super", "switch", "this", "throw", "true", "try", "type", "typeof", "undefined", "var", "void", "while", "yield"]),
  py: new Set(["and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "match", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True", "try", "while", "with", "yield"]),
  go: new Set(["break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough", "false", "for", "func", "go", "goto", "if", "import", "interface", "map", "nil", "package", "range", "return", "select", "struct", "switch", "true", "type", "var"]),
};

const CODE_TYPES = {
  ts: new Set(["any", "bigint", "boolean", "never", "number", "object", "string", "symbol", "unknown", "void"]),
  py: new Set(["Any", "bool", "bytes", "dict", "float", "int", "list", "object", "set", "str", "tuple"]),
  go: new Set(["any", "bool", "byte", "error", "float32", "float64", "int", "int8", "int16", "int32", "int64", "rune", "string", "uint", "uint8", "uint16", "uint32", "uint64"]),
};

// Small per-target highlighter: comments, strings, numbers, keywords, and
// capitalized type names. Runs at render time so the page stays one file.
function highlightCode(code, lang) {
  const keywords = CODE_KEYWORDS[lang] || new Set();
  const lineComment = lang === "py" ? "#" : "//";
  const out = [];
  let i = 0;
  while (i < code.length) {
    const rest = code.slice(i);
    if (code.startsWith(lineComment, i)) {
      const end = code.indexOf("\n", i);
      const stop = end < 0 ? code.length : end;
      out.push(`<span class="tok-comment">${esc(code.slice(i, stop))}</span>`);
      i = stop;
      continue;
    }
    if (lang !== "py" && code.startsWith("/*", i)) {
      const close = code.indexOf("*/", i + 2);
      const stop = close < 0 ? code.length : close + 2;
      out.push(`<span class="tok-comment">${esc(code.slice(i, stop))}</span>`);
      i = stop;
      continue;
    }
    const ch = code[i];
    if (ch === '"' || ch === "'" || ch === "`") {
      let j = i + 1;
      while (j < code.length) {
        if (code[j] === "\\") { j += 2; continue; }
        if (code[j] === ch) { j += 1; break; }
        if (ch !== "`" && code[j] === "\n") break;
        j += 1;
      }
      out.push(`<span class="tok-str">${esc(code.slice(i, j))}</span>`);
      i = j;
      continue;
    }
    const num = /^(?:0[xX][0-9a-fA-F_]+|\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?n?)/.exec(rest);
    if (num) {
      out.push(`<span class="tok-num">${esc(num[0])}</span>`);
      i += num[0].length;
      continue;
    }
    const id = /^[A-Za-z_$][A-Za-z0-9_$]*/.exec(rest);
    if (id) {
      const word = id[0];
      const types = CODE_TYPES[lang] || new Set();
      const cls = keywords.has(word)
        ? "tok-kw"
        : types.has(word) || /^[A-Z]/.test(word)
          ? "tok-type"
          : null;
      out.push(cls ? `<span class="${cls}">${esc(word)}</span>` : esc(word));
      i += word.length;
      continue;
    }
    out.push(esc(ch));
    i += 1;
  }
  return out.join("");
}

function renderCase(id) {
  const c = cases.get(id);
  if (!c) {
    main.innerHTML = `<h2>Case not found</h2><p class="why">Pick a case in the sidebar.</p>`;
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
  tag.textContent = feature ? `feature: ${feature.title}${feature.spec ? " · " + feature.spec : ""}` : c.feature;
  main.appendChild(tag);
  if (c.why) {
    const why = document.createElement("p");
    why.className = "why";
    why.textContent = c.why;
    main.appendChild(why);
  }
  const badges = document.createElement("div");
  badges.className = "badges";
  badges.innerHTML = statusBadges(c) + `<span class="badge">${c.kind === "diagnostic" ? "diagnostic" : "executable"}</span>`;
  main.appendChild(badges);

  main.appendChild(block("source (.lt)", c.sourceHtml, c.source));

  if (c.kind === "diagnostic") {
    const diag = document.createElement("div");
    diag.className = "diag ok";
    diag.textContent = c.diagnostic || "";
    main.appendChild(block("diagnostic (must fail)"));
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
      b.textContent = TARGET_LABEL[t];
      b.className = t === current ? "active" : "";
      b.onclick = () => { current = t; draw(); };
      tabs.appendChild(b);
    }
    panes.innerHTML = "";
    const code = c.outputs[current] || "";
    panes.appendChild(block("output " + current, `<code>${highlightCode(code, current)}</code>`, code));
    const usage = (c.usage && c.usage[current]) || "";
    if (usage) {
      panes.appendChild(block("usage · " + TARGET_LABEL[current], `<code>${highlightCode(usage, current)}</code>`, usage));
      const lines = (c.usageOutput && c.usageOutput[current]) || [];
      if (lines.length) {
        const text = lines.join("\n");
        const out = block("stdout · " + TARGET_LABEL[current], `<code>${esc(text)}</code>`, text);
        out.querySelector("pre").classList.add("stdout");
        panes.appendChild(out);
      } else {
        const note = document.createElement("div");
        note.className = "block empty";
        note.textContent = `no recorded ${TARGET_LABEL[current]} output yet; run scripts/playground.sh`;
        panes.appendChild(note);
      }
    }
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
