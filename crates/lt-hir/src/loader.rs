//! Module loading: parse files, follow imports, detect cycles.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use lt_syntax::span::line_col_at;
use lt_syntax::{parse, Diagnostic, Span};

use crate::program::{ImportInfo, Module, ModuleId, Program};

pub trait SourceLoader {
    fn read(&mut self, path: &Path) -> Option<String>;
}

pub struct FsLoader;

impl SourceLoader for FsLoader {
    fn read(&mut self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
}

#[derive(Default)]
pub struct MapLoader {
    pub files: HashMap<PathBuf, String>,
}

impl MapLoader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, path: &str, source: &str) {
        self.files.insert(normalize(PathBuf::from(path)), source.to_string());
    }
}

impl SourceLoader for MapLoader {
    fn read(&mut self, path: &Path) -> Option<String> {
        self.files.get(&normalize(path.to_path_buf())).cloned()
    }
}

pub struct LoadResult {
    pub program: Program,
    pub diagnostics: Vec<(PathBuf, Diagnostic)>,
}

/// Load every entry and everything it imports into one program.
pub fn load(entries: &[PathBuf], loader: &mut dyn SourceLoader) -> LoadResult {
    let mut state = Loader {
        loader,
        modules: Vec::new(),
        by_path: HashMap::new(),
        stack: Vec::new(),
        diagnostics: Vec::new(),
    };
    for entry in entries {
        state.load(normalize(entry.clone()));
    }
    LoadResult {
        program: Program {
            modules: state.modules,
        },
        diagnostics: state.diagnostics,
    }
}

struct Loader<'a> {
    loader: &'a mut dyn SourceLoader,
    modules: Vec<Module>,
    by_path: HashMap<PathBuf, ModuleId>,
    stack: Vec<PathBuf>,
    diagnostics: Vec<(PathBuf, Diagnostic)>,
}

impl<'a> Loader<'a> {
    fn load(&mut self, path: PathBuf) -> Option<ModuleId> {
        if let Some(position) = self.stack.iter().position(|p| p == &path) {
            let mut chain: Vec<String> = self.stack[position..]
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            chain.push(path.display().to_string());
            self.diagnostics.push((
                path.clone(),
                Diagnostic::error(
                    format!("import cycle: {}", chain.join(" -> ")),
                    Span::default(),
                    1,
                    1,
                ),
            ));
            return None;
        }
        if let Some(&id) = self.by_path.get(&path) {
            return Some(id);
        }

        let Some(source) = self.loader.read(&path) else {
            self.diagnostics.push((
                path.clone(),
                Diagnostic::error(
                    format!("cannot read `{}`", path.display()),
                    Span::default(),
                    1,
                    1,
                ),
            ));
            return None;
        };

        let parsed = parse(&source);
        self.diagnostics
            .extend(parsed.diagnostics.into_iter().map(|d| (path.clone(), d)));

        let id = self.modules.len();
        self.by_path.insert(path.clone(), id);
        self.modules.push(Module {
            id,
            path: path.clone(),
            source,
            ast: parsed.file,
            imports: Vec::new(),
            classes: Vec::new(),
            enums: Vec::new(),
        });

        self.stack.push(path.clone());
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let imports = self.modules[id].ast.imports.clone();
        let source = self.modules[id].source.clone();

        for decl in imports {
            if decl.path.starts_with("http://") || decl.path.starts_with("https://") {
                let loc = line_col_at(&source, decl.span.start);
                self.diagnostics.push((
                    path.clone(),
                    Diagnostic::error(
                        format!(
                            "remote import `{}` needs the package manager, which lands in M7; \
                             vendor the file and import it with a relative path for now",
                            decl.path
                        ),
                        decl.span,
                        loc.line,
                        loc.col,
                    ),
                ));
                continue;
            }
            if !decl.path.starts_with("./") && !decl.path.starts_with("../") {
                let loc = line_col_at(&source, decl.span.start);
                self.diagnostics.push((
                    path.clone(),
                    Diagnostic::error(
                        format!(
                            "import path `{}` must start with `./` or `../`",
                            decl.path
                        ),
                        decl.span,
                        loc.line,
                        loc.col,
                    ),
                ));
                continue;
            }

            let target = normalize(parent.join(&decl.path));
            let Some(child) = self.load(target) else {
                continue;
            };

            if self.modules[id].imports.iter().any(|i| i.name == decl.name) {
                let loc = line_col_at(&source, decl.span.start);
                self.diagnostics.push((
                    path.clone(),
                    Diagnostic::error(
                        format!("`{}` is imported twice in this file", decl.name),
                        decl.span,
                        loc.line,
                        loc.col,
                    ),
                ));
                continue;
            }

            self.modules[id].imports.push(ImportInfo {
                name: decl.name,
                module: child,
                span: decl.span,
            });
        }

        self.stack.pop();
        Some(id)
    }
}

/// Lexically normalize a path: drop `.`, resolve `..`, keep it relative.
pub fn normalize(path: PathBuf) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}
