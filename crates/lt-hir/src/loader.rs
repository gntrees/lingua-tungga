//! Module loading: parse files, follow imports (local and remote), detect cycles.

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
        self.files
            .insert(normalize(PathBuf::from(path)), source.to_string());
    }
}

impl SourceLoader for MapLoader {
    fn read(&mut self, path: &Path) -> Option<String> {
        self.files.get(&normalize(path.to_path_buf())).cloned()
    }
}

/// A remote file that has been fetched and pinned.
#[derive(Debug, Clone)]
pub struct ResolvedRemote {
    /// URL containing the content hash or commit, used for further imports.
    pub canonical_url: String,
    pub content: String,
    pub sha256: String,
}

/// Resolves `https://` imports. The package manager implements this.
pub trait RemoteResolver {
    fn resolve(&mut self, url: &str) -> Result<ResolvedRemote, String>;
}

/// Default resolver: remote imports are an error.
pub struct NoRemote;

impl RemoteResolver for NoRemote {
    fn resolve(&mut self, _url: &str) -> Result<ResolvedRemote, String> {
        Err("remote imports need the package manager (run `lt build` with a lockfile-aware command)".to_string())
    }
}

pub struct LoadResult {
    pub program: Program,
    pub diagnostics: Vec<(PathBuf, Diagnostic)>,
}

/// Packages produced by one file, keyed by package name.
type FilePackages = HashMap<String, ModuleId>;

pub fn load(entries: &[PathBuf], loader: &mut dyn SourceLoader) -> LoadResult {
    load_with_remote(entries, loader, &mut NoRemote)
}

pub fn load_with_remote(
    entries: &[PathBuf],
    loader: &mut dyn SourceLoader,
    remote: &mut dyn RemoteResolver,
) -> LoadResult {
    let mut state = Loader {
        loader,
        remote,
        modules: Vec::new(),
        by_path: HashMap::new(),
        by_url: HashMap::new(),
        by_name: HashMap::new(),
        stack: Vec::new(),
        diagnostics: Vec::new(),
    };
    for entry in entries {
        state.load_local(normalize(entry.clone()));
    }
    LoadResult {
        program: Program {
            modules: state.modules,
        },
        diagnostics: state.diagnostics,
    }
}

enum Parent {
    Local(PathBuf),
    Remote(String),
}

struct Loader<'a> {
    loader: &'a mut dyn SourceLoader,
    remote: &'a mut dyn RemoteResolver,
    modules: Vec<Module>,
    /// A file yields one module per package.
    by_path: HashMap<PathBuf, FilePackages>,
    by_url: HashMap<String, FilePackages>,
    /// Package names are unique per build, so a name identifies a module after load.
    by_name: HashMap<String, ModuleId>,
    stack: Vec<PathBuf>,
    diagnostics: Vec<(PathBuf, Diagnostic)>,
}

impl<'a> Loader<'a> {
    fn load_local(&mut self, path: PathBuf) -> Option<FilePackages> {
        if let Some(position) = self.stack.iter().position(|p| p == &path) {
            self.cycle_error(position, &path);
            return None;
        }
        if let Some(packages) = self.by_path.get(&path) {
            return Some(packages.clone());
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

        let parent = Parent::Local(path.parent().map(Path::to_path_buf).unwrap_or_default());
        let packages = self.load_file(path.clone(), source, None, parent);
        self.by_path.insert(path, packages.clone());
        Some(packages)
    }

    fn load_remote(&mut self, url: &str) -> Option<FilePackages> {
        let resolved = match self.remote.resolve(url) {
            Ok(resolved) => resolved,
            Err(message) => {
                self.diagnostics.push((
                    PathBuf::from(url),
                    Diagnostic::error(message, Span::default(), 1, 1),
                ));
                return None;
            }
        };

        if let Some(packages) = self.by_url.get(&resolved.canonical_url) {
            return Some(packages.clone());
        }

        let identity = remote_identity(&resolved.canonical_url, &resolved.sha256);
        if let Some(position) = self.stack.iter().position(|p| p == &identity) {
            self.cycle_error(position, &identity);
            return None;
        }

        let parent = Parent::Remote(resolved.canonical_url.clone());
        let packages = self.load_file(
            identity.clone(),
            resolved.content,
            Some(resolved.canonical_url.clone()),
            parent,
        );
        self.by_url.insert(resolved.canonical_url, packages.clone());
        Some(packages)
    }

    /// Parse one file and mint one module per `package` block, then resolve the
    /// file's imports once and share them across those modules.
    fn load_file(
        &mut self,
        path: PathBuf,
        source: String,
        origin: Option<String>,
        parent: Parent,
    ) -> FilePackages {
        let parsed = parse(&source);
        self.diagnostics
            .extend(parsed.diagnostics.into_iter().map(|d| (path.clone(), d)));

        let mut ids: FilePackages = HashMap::new();
        for package in &parsed.file.packages {
            let loc = line_col_at(&source, package.span.start);
            if ids.contains_key(&package.name) {
                self.diagnostics.push((
                    path.clone(),
                    Diagnostic::error(
                        format!(
                            "`package {}` is declared more than once in this file",
                            package.name
                        ),
                        package.span,
                        loc.line,
                        loc.col,
                    ),
                ));
                continue;
            }
            if let Some(existing) = self.by_name.get(&package.name) {
                let other = self.modules[*existing].path.display().to_string();
                self.diagnostics.push((
                    path.clone(),
                    Diagnostic::error(
                        format!(
                            "package `{}` is already declared in `{other}`; package names must be unique in a build",
                            package.name
                        ),
                        package.span,
                        loc.line,
                        loc.col,
                    ),
                ));
                continue;
            }

            let id = self.modules.len();
            self.modules.push(Module {
                id,
                path: path.clone(),
                origin: origin.clone(),
                source: source.clone(),
                name: package.name.clone(),
                ast: package.clone(),
                imports: Vec::new(),
                structs: Vec::new(),
                enums: Vec::new(),
                functions: Vec::new(),
                bindings: Vec::new(),
                tests: Vec::new(),
            });
            self.by_name.insert(package.name.clone(), id);
            ids.insert(package.name.clone(), id);
        }

        if ids.is_empty() {
            return ids;
        }

        self.stack.push(path.clone());
        let mut resolved: Vec<ImportInfo> = Vec::new();
        let mut seen_bindings: Vec<String> = Vec::new();

        for decl in &parsed.file.imports {
            let child = if decl.path.starts_with("http://") || decl.path.starts_with("https://") {
                self.load_remote(&decl.path)
            } else if !decl.path.starts_with("./") && !decl.path.starts_with("../") {
                let loc = line_col_at(&source, decl.span.start);
                self.diagnostics.push((
                    path.clone(),
                    Diagnostic::error(
                        format!(
                            "import path `{}` must start with `./`, `../`, or `https://`",
                            decl.path
                        ),
                        decl.span,
                        loc.line,
                        loc.col,
                    ),
                ));
                None
            } else {
                match &parent {
                    Parent::Local(dir) => self.load_local(normalize(dir.join(&decl.path))),
                    Parent::Remote(base) => self.load_remote(&join_url(base, &decl.path)),
                }
            };

            let Some(packages) = child else {
                continue;
            };
            let Some(module) = packages.get(&decl.package).copied() else {
                let loc = line_col_at(&source, decl.span.start);
                let available: Vec<String> = {
                    let mut names: Vec<String> = packages.keys().cloned().collect();
                    names.sort();
                    names
                };
                self.diagnostics.push((
                    path.clone(),
                    Diagnostic::error(
                        format!(
                            "`{}` does not declare `package {}` (found: {})",
                            decl.path,
                            decl.package,
                            if available.is_empty() {
                                "no packages".to_string()
                            } else {
                                available.join(", ")
                            }
                        ),
                        decl.span,
                        loc.line,
                        loc.col,
                    ),
                ));
                continue;
            };

            let binding = decl.binding().to_string();
            if seen_bindings.contains(&binding) {
                let loc = line_col_at(&source, decl.span.start);
                self.diagnostics.push((
                    path.clone(),
                    Diagnostic::error(
                        format!("`{binding}` is imported twice in this file"),
                        decl.span,
                        loc.line,
                        loc.col,
                    ),
                ));
                continue;
            }
            seen_bindings.push(binding.clone());

            resolved.push(ImportInfo {
                package: decl.package.clone(),
                binding,
                module,
                items: decl
                    .items
                    .iter()
                    .map(|item| crate::program::ImportItemInfo {
                        name: item.name.clone(),
                        binding: item.binding().to_string(),
                        span: item.span,
                    })
                    .collect(),
                span: decl.span,
            });
        }

        for id in ids.values() {
            self.modules[*id].imports = resolved.clone();
        }

        self.stack.pop();
        ids
    }

    fn cycle_error(&mut self, position: usize, path: &Path) {
        let mut chain: Vec<String> = self.stack[position..]
            .iter()
            .map(|p| self.display_path(p))
            .collect();
        chain.push(self.display_path(path));
        self.diagnostics.push((
            path.to_path_buf(),
            Diagnostic::error(
                format!("import cycle: {}", chain.join(" -> ")),
                Span::default(),
                1,
                1,
            ),
        ));
    }

    fn display_path(&self, path: &Path) -> String {
        self.modules
            .iter()
            .find(|module| module.path == path)
            .and_then(|module| module.origin.clone())
            .unwrap_or_else(|| path.display().to_string())
    }
}

/// Stable synthetic path for a remote module, used for output naming and cycles.
fn remote_identity(canonical_url: &str, sha256: &str) -> PathBuf {
    let (host, path) = split_url(canonical_url);
    let host: String = host
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    let digest = &sha256[..sha256.len().min(8)];
    PathBuf::from(format!(
        "remote/{host}/{digest}/{}",
        path.trim_start_matches('/')
    ))
}

fn split_url(url: &str) -> (String, String) {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    match without_scheme.split_once('/') {
        Some((host, path)) => (host.to_string(), path.to_string()),
        None => (without_scheme.to_string(), String::new()),
    }
}

/// Join a relative import against a canonical URL, normalizing `.` and `..`.
pub fn join_url(base: &str, relative: &str) -> String {
    if relative.starts_with("http://") || relative.starts_with("https://") {
        return relative.to_string();
    }
    let prefix = if base.starts_with("https://") {
        "https://"
    } else {
        "http://"
    };
    let without_scheme = base
        .strip_prefix("https://")
        .or_else(|| base.strip_prefix("http://"))
        .unwrap_or(base);
    let mut segments: Vec<&str> = without_scheme.split('/').collect();
    segments.pop();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if segments.len() > 1 {
                    segments.pop();
                }
            }
            other => segments.push(other),
        }
    }
    format!("{prefix}{}", segments.join("/"))
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
