//! Static playground generator.
//!
//! Cases live in `playground/cases/<group>/<id>.lt` with a sibling
//! `<id>.toml`; diagnostic cases live in `playground/diagnostics/`.
//! The output is a single self-contained `index.html`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lt_hir::{check_entries, MapLoader, Program};
use lt_ir::lower_program;
use lt_syntax::{tokenize_full, TokenKind};
use serde::Deserialize;

mod html;

use lt_ir::usage_block;

#[derive(Debug, Clone, Deserialize)]
pub struct Feature {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub spec: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FeatureFile {
    #[serde(default)]
    feature: Vec<Feature>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct CaseMeta {
    #[serde(default)]
    title: String,
    #[serde(default)]
    feature: String,
    #[serde(default)]
    why: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    expect: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Case {
    pub id: String,
    pub title: String,
    pub why: String,
    pub feature: String,
    pub kind: String,
    pub source: String,
    pub source_html: String,
    pub outputs: BTreeMap<String, String>,
    /// Per-target runnable program that calls the emitted library.
    pub usage: BTreeMap<String, String>,
    /// Per-target standard output of [`usage`], recorded by `scripts/playground.sh`.
    pub usage_output: BTreeMap<String, Vec<String>>,
    pub diagnostic: Option<String>,
    pub status: BTreeMap<String, String>,
}

pub struct Options {
    pub root: PathBuf,
    pub out: PathBuf,
    pub allow_stale: bool,
}

#[derive(Debug, Default)]
pub struct Summary {
    pub cases: usize,
    pub diagnostics: usize,
    pub features: usize,
    pub stale_evidence: bool,
    pub output: PathBuf,
}

#[derive(Deserialize, Default)]
struct Evidence {
    #[serde(default)]
    generated_at: String,
    #[serde(default)]
    git: Option<serde_json::Value>,
    #[serde(default)]
    targets: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    cases: BTreeMap<String, BTreeMap<String, bool>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct UsageRun {
    #[serde(default)]
    status: String,
    #[serde(default)]
    lines: Vec<String>,
}

#[derive(Deserialize, Default)]
struct UsageFile {
    #[serde(default)]
    cases: BTreeMap<String, BTreeMap<String, UsageRun>>,
}

pub fn generate(options: &Options) -> Result<Summary, String> {
    let features_path = options.root.join("features.toml");
    let feature_text = std::fs::read_to_string(&features_path)
        .map_err(|error| format!("{}: {error}", features_path.display()))?;
    let feature_file: FeatureFile = toml::from_str(&feature_text)
        .map_err(|error| format!("{}: {error}", features_path.display()))?;
    if feature_file.feature.is_empty() {
        return Err("playground/features.toml has no [[feature]] entries".to_string());
    }

    let mut cases = Vec::new();
    load_dir(&options.root.join("cases"), "", "run", &mut cases)?;
    load_dir(
        &options.root.join("diagnostics"),
        "diag/",
        "diagnostic",
        &mut cases,
    )?;
    cases.sort_by(|a, b| a.id.cmp(&b.id));

    let known: BTreeSet<String> = feature_file.feature.iter().map(|f| f.id.clone()).collect();
    let mut covered: BTreeSet<String> = BTreeSet::new();
    for case in &cases {
        if !known.contains(&case.feature) {
            return Err(format!(
                "case `{}` references unknown feature `{}`",
                case.id, case.feature
            ));
        }
        covered.insert(case.feature.clone());
    }
    let missing: Vec<&str> = feature_file
        .feature
        .iter()
        .filter(|feature| !covered.contains(&feature.id))
        .map(|feature| feature.id.as_str())
        .collect();
    if !missing.is_empty() {
        return Err(format!("features without a case: {}", missing.join(", ")));
    }

    let evidence_path = options.root.join("evidence.json");
    let evidence: Option<Evidence> = std::fs::read_to_string(&evidence_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let usage_path = options.root.join("usage.json");
    let usage: UsageFile = std::fs::read_to_string(&usage_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();

    let run_ids: BTreeSet<String> = cases
        .iter()
        .filter(|case| case.kind == "run")
        .map(|case| case.id.clone())
        .collect();

    let mut stale = false;
    if let Some(evidence) = &evidence {
        let recorded: BTreeSet<String> = evidence.cases.keys().cloned().collect();
        if recorded != run_ids {
            stale = true;
            if !options.allow_stale {
                return Err(format!(
                    "evidence.json does not match the current cases ({} recorded, {} current); run scripts/playground.sh",
                    recorded.len(),
                    run_ids.len()
                ));
            }
        }
    }

    let mut outputs = Vec::new();
    let mut diagnostics = 0;
    for case in &mut cases {
        if case.kind == "diagnostic" {
            diagnostics += 1;
            case.status.insert("diag".to_string(), "ok".to_string());
            outputs.push(case.clone());
            continue;
        }
        if let Some(evidence) = &evidence {
            if let Some(recorded) = evidence.cases.get(&case.id) {
                for target in ["ts", "py", "go"] {
                    let status = if *recorded.get(target).unwrap_or(&false) {
                        "pass"
                    } else {
                        "fail"
                    };
                    case.status.insert(target.to_string(), status.to_string());
                }
            }
        }
        for target in ["ts", "py", "go"] {
            case.status
                .entry(target.to_string())
                .or_insert_with(|| "not-run".to_string());
        }
        if let Some(recorded) = usage.cases.get(&case.id) {
            for (target, run) in recorded {
                case.usage_output.insert(target.clone(), run.lines.clone());
                if run.status == "fail" {
                    case.status.insert(target.clone(), "fail".to_string());
                }
            }
        }
        outputs.push(case.clone());
    }

    let summary = Summary {
        cases: run_ids.len(),
        diagnostics,
        features: feature_file.feature.len(),
        stale_evidence: stale,
        output: options.out.join("index.html"),
    };

    let document = html::Document {
        features: feature_file.feature.clone(),
        cases: outputs,
        evidence_generated_at: evidence
            .as_ref()
            .map(|evidence| evidence.generated_at.clone())
            .unwrap_or_default(),
        evidence_targets: evidence
            .as_ref()
            .map(|evidence| serde_json::Value::Object(evidence.targets.clone()))
            .unwrap_or(serde_json::Value::Null),
        evidence_git: evidence
            .as_ref()
            .and_then(|evidence| evidence.git.clone())
            .unwrap_or(serde_json::Value::Null),
        stale,
    };

    std::fs::create_dir_all(&options.out)
        .map_err(|error| format!("{}: {error}", options.out.display()))?;
    std::fs::write(&summary.output, html::render(&document))
        .map_err(|error| format!("{}: {error}", summary.output.display()))?;
    Ok(summary)
}

fn load_dir(dir: &Path, prefix: &str, kind: &str, cases: &mut Vec<Case>) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();

    for path in entries {
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            load_dir(&path, &format!("{prefix}{name}/"), kind, cases)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("lt") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| format!("{}: invalid file name", path.display()))?
            .to_string();
        // `_name.lt` files are import targets for a case, never cases themselves.
        if stem.starts_with('_') {
            continue;
        }
        let id = format!("{prefix}{stem}");
        cases.push(build_case(&path, &stem, &id, kind)?);
    }
    Ok(())
}

fn build_case(path: &Path, stem: &str, id: &str, kind: &str) -> Result<Case, String> {
    let source =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let meta_path = path.with_extension("toml");
    let meta_text = std::fs::read_to_string(&meta_path).map_err(|error| {
        format!(
            "{}: {error} (every case needs a sidecar .toml)",
            meta_path.display()
        )
    })?;
    let meta: CaseMeta =
        toml::from_str(&meta_text).map_err(|error| format!("{}: {error}", meta_path.display()))?;
    let effective_kind = if meta.kind.is_empty() {
        kind
    } else {
        meta.kind.as_str()
    };

    let mut loader = MapLoader::new();
    loader.add("case.lt", &source);
    // Sibling `_name.lt` files exist only as import targets for this case.
    if let Some(dir) = path.parent() {
        let mut helpers: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|error| format!("{}: {error}", dir.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("lt"))
            .collect();
        helpers.sort();
        for helper in helpers {
            let Some(name) = helper.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with('_') {
                continue;
            }
            let text = std::fs::read_to_string(&helper)
                .map_err(|error| format!("{}: {error}", helper.display()))?;
            loader.add(name, &text);
        }
    }
    let checked = check_entries(&[PathBuf::from("case.lt")], &mut loader);

    let mut outputs = BTreeMap::new();
    let mut usage = BTreeMap::new();
    let mut diagnostic = None;

    if effective_kind == "diagnostic" {
        let mut messages: Vec<String> = checked
            .diagnostics
            .iter()
            .map(|file| file.diagnostic.message.clone())
            .collect();
        if checked.is_ok() {
            let (_, lower_diagnostics) = lower_program(
                &checked.program,
                &checked.types,
                module_names(&checked.program, stem),
            );
            messages.extend(
                lower_diagnostics
                    .into_iter()
                    .map(|(_, diagnostic)| diagnostic.message),
            );
        }
        if messages.is_empty() {
            return Err(format!(
                "{}: expected a compile error, but it type checked",
                id
            ));
        }
        let expect = meta
            .expect
            .as_deref()
            .ok_or_else(|| format!("{}: diagnostic cases need `expect = \"...\"`", id))?;
        let found = messages
            .iter()
            .find(|message| message.contains(expect))
            .cloned();
        let Some(message) = found else {
            let actual: Vec<String> = checked
                .diagnostics
                .iter()
                .map(|file| file.diagnostic.message.clone())
                .collect();
            return Err(format!(
                "{}: no diagnostic contains `{expect}`; got {:?}",
                id, actual
            ));
        };
        diagnostic = Some(message);
    } else {
        if !checked.is_ok() {
            let messages: Vec<String> = checked
                .diagnostics
                .iter()
                .map(|file| file.diagnostic.message.clone())
                .collect();
            return Err(format!("{}: does not type check: {:?}", id, messages));
        }
        let (ir, lower_diagnostics) = lower_program(
            &checked.program,
            &checked.types,
            module_names(&checked.program, stem),
        );
        if !lower_diagnostics.is_empty() {
            return Err(format!("{}: lowering failed", id));
        }
        let ts = lt_emit_ts::emit_package(&ir);
        let py = lt_emit_py::emit_package(&ir);
        let go = lt_emit_go::emit_package(&ir);
        outputs.insert("ts".to_string(), pick(ts, &stems("src", stem, "ts")));
        outputs.insert("py".to_string(), pick(py, &stems("gen", stem, "py")));
        outputs.insert("go".to_string(), pick(go, &format!("gen/{stem}/{stem}.go")));

        let module = ir
            .modules
            .iter()
            .find(|module| module.name == stem)
            .or_else(|| ir.modules.first())
            .ok_or_else(|| format!("{}: lowering produced no module", id))?;
        let exported = module.functions.iter().filter(|f| f.is_export).count()
            + module.bindings.iter().filter(|b| b.is_export).count()
            + module.structs.iter().filter(|s| s.is_export).count()
            + module.enums.iter().filter(|e| e.is_export).count();
        if exported == 0 {
            return Err(format!(
                "{}: exports nothing; every executable case needs at least one `export` declaration",
                id
            ));
        }
        if usage_block(module).is_none() {
            return Err(format!(
                "{}: has no test statements; usage examples are derived from `test` blocks",
                id
            ));
        }
        usage.insert(
            "ts".to_string(),
            pick(lt_emit_ts::emit_usage(&ir), &format!("src/{stem}.usage.ts")),
        );
        usage.insert(
            "py".to_string(),
            pick(lt_emit_py::emit_usage(&ir), &format!("usage/{stem}.py")),
        );
        usage.insert(
            "go".to_string(),
            pick(
                lt_emit_go::emit_usage(&ir),
                &format!("usage/{stem}/main.go"),
            ),
        );
        for (target, code) in &usage {
            if code.is_empty() {
                return Err(format!("{}: no usage program emitted for `{target}`", id));
            }
        }
    }

    Ok(Case {
        id: id.to_string(),
        title: if meta.title.is_empty() {
            stem.replace('_', " ")
        } else {
            meta.title.clone()
        },
        why: meta.why.clone(),
        feature: meta.feature.clone(),
        kind: effective_kind.to_string(),
        source_html: highlight(&source),
        source,
        outputs,
        usage,
        usage_output: BTreeMap::new(),
        diagnostic,
        status: BTreeMap::new(),
    })
}

/// One output name per module, in module order. A case that imports pulls extra
/// modules into the program, and every one of them needs a name. The case itself
/// keeps the file stem so its output path is predictable; imported packages keep
/// their declared name.
fn module_names(program: &Program, stem: &str) -> Vec<String> {
    program
        .modules
        .iter()
        .map(|module| {
            if module.path == Path::new("case.lt") {
                stem.to_string()
            } else {
                module.name.clone()
            }
        })
        .collect()
}

fn stems(dir: &str, stem: &str, ext: &str) -> String {
    format!("{dir}/{stem}.{ext}")
}

fn pick(files: Vec<impl EmittedLike>, path: &str) -> String {
    files
        .into_iter()
        .find(|file| file.path_string() == path)
        .map(|file| file.contents_string())
        .unwrap_or_default()
}

trait EmittedLike {
    fn path_string(&self) -> String;
    fn contents_string(&self) -> String;
}

impl EmittedLike for lt_emit_ts::EmittedFile {
    fn path_string(&self) -> String {
        self.path.to_string_lossy().replace('\\', "/")
    }
    fn contents_string(&self) -> String {
        self.contents.clone()
    }
}

impl EmittedLike for lt_emit_py::EmittedFile {
    fn path_string(&self) -> String {
        self.path.to_string_lossy().replace('\\', "/")
    }
    fn contents_string(&self) -> String {
        self.contents.clone()
    }
}

impl EmittedLike for lt_emit_go::EmittedFile {
    fn path_string(&self) -> String {
        self.path.to_string_lossy().replace('\\', "/")
    }
    fn contents_string(&self) -> String {
        self.contents.clone()
    }
}

/// Minimal HTML highlighting from the real lexer.
fn highlight(source: &str) -> String {
    let lexed = tokenize_full(source);
    let mut marks: Vec<(usize, usize, &'static str)> = Vec::new();
    for comment in &lexed.comments {
        marks.push((comment.start, comment.end, "comment"));
    }
    for token in &lexed.tokens {
        let class = match &token.kind {
            TokenKind::Str(_) => Some("str"),
            TokenKind::TemplateText(_) | TokenKind::JsxText(_) => Some("str"),
            TokenKind::Int | TokenKind::Float => Some("num"),
            TokenKind::Ident(name) if name.chars().next().is_some_and(|c| c.is_uppercase()) => {
                Some("type")
            }
            kind if !kind.spelling().is_empty() => Some("kw"),
            _ => None,
        };
        if let Some(class) = class {
            marks.push((token.span.start, token.span.end, class));
        }
    }
    marks.sort_by_key(|(start, _, _)| *start);

    let mut out = String::new();
    let mut cursor = 0;
    for (start, end, class) in marks {
        if start < cursor {
            continue;
        }
        out.push_str(&escape_html(&source[cursor..start]));
        out.push_str(&format!(
            "<span class=\"tok-{class}\">{}</span>",
            escape_html(&source[start..end])
        ));
        cursor = end;
    }
    out.push_str(&escape_html(&source[cursor..]));
    out
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}
