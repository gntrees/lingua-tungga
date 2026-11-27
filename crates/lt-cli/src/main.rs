//! `lt` — Lingua Tungga command line interface.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::exit;

use std::collections::HashMap;

use lt_hir::{check_entries_with_remote, normalize, FsLoader};
use lt_ir::lower_program;
use lt_ir::IrProgram;
use lt_pkg::{Manager, Mode};
use lt_syntax::{format_file_with_comments, parse, tokenize_full, Diagnostic};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    exit(run(&args));
}

fn run(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        None => {
            usage();
            2
        }
        Some("help") | Some("--help") | Some("-h") => {
            usage();
            0
        }
        Some("check") => cmd_check(&args[1..]),
        Some("fmt") => cmd_fmt(&args[1..]),
        Some("ast") => cmd_ast(&args[1..]),
        Some("build") => cmd_build(&args[1..]),
        Some("test") => cmd_test(&args[1..]),
        Some("ir") => cmd_ir(&args[1..]),
        Some("update") => cmd_update(&args[1..]),
        Some("vendor") => cmd_vendor(&args[1..]),
        Some("playground") => cmd_playground(&args[1..]),
        Some(other) => {
            eprintln!("lt: unknown command `{other}`");
            usage();
            2
        }
    }
}

fn usage() {
    eprintln!(
        "lt — Lingua Tungga toolchain

usage:
  lt check <path>...        resolve imports and type check
  lt fmt [--check] <path>   format files in place (or check formatting)
  lt ast <path>             parse and dump the AST (debugging)
  lt build -t ts|py|go <entry>    compile to a package (default out/<target>)
  lt test -t ts|py|go <entry>...  emit + run the conformance suite
                               (--usage also emits the calling examples)
  lt ir <entry>             lower to IR and dump it (debugging)
  lt update <entry>...      re-resolve URL imports and rewrite lt.lock
  lt vendor <entry>...      copy locked remote files into vendor/
  lt playground             generate the static playground (out/playground)

paths may be files or directories (searched recursively for *.lt)"
    );
}

fn collect_paths(inputs: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for input in inputs {
        let path = PathBuf::from(input);
        if path.is_dir() {
            walk(&path, &mut out).map_err(|error| format!("{}: {error}", path.display()))?;
        } else if path.is_file() {
            out.push(path);
        } else {
            return Err(format!("{}: no such file or directory", path.display()));
        }
    }
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == OsStr::new("lt")) {
            out.push(path);
        }
    }
    Ok(())
}

fn report(path: &Path, src: &str, diagnostics: &[Diagnostic]) {
    let name = path.display().to_string();
    for diagnostic in diagnostics {
        eprintln!("{}", diagnostic.render(&name, src));
    }
}

fn read_source(path: &Path) -> Result<String, i32> {
    std::fs::read_to_string(path).map_err(|error| {
        eprintln!("lt: {}: {error}", path.display());
        1
    })
}

fn cmd_check(inputs: &[String]) -> i32 {
    let paths = match collect_paths(inputs) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("lt: {error}");
            return 2;
        }
    };
    if paths.is_empty() {
        eprintln!("lt: no .lt files found");
        return 2;
    }

    // `check` accepts unrelated directories, so it does not need a single
    // project root (that is only required for output naming).
    let root = normalize(
        paths
            .first()
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    );
    let mut manager = Manager::new(
        if root == Path::new(".") {
            Path::new("")
        } else {
            &root
        },
        Mode::Locked,
    );
    let mut loader = FsLoader;
    let out = check_entries_with_remote(&paths, &mut loader, &mut manager);
    if let Err(error) = manager.save() {
        eprintln!("lt: {error}");
        return 1;
    }

    if out.diagnostics.is_empty() {
        for path in &paths {
            println!("ok  {}", path.display());
        }
        return 0;
    }

    print_file_diagnostics(&out);
    1
}

/// Deepest directory that contains every entry, so output names stay relative.
fn common_root(entries: &[PathBuf]) -> PathBuf {
    let mut root: Option<PathBuf> = None;
    for entry in entries {
        let parent = normalize(
            entry
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")),
        );
        root = Some(match root {
            None => parent,
            Some(current) => {
                let parts: Vec<&std::ffi::OsStr> =
                    current.components().map(|c| c.as_os_str()).collect();
                let other: Vec<&std::ffi::OsStr> =
                    parent.components().map(|c| c.as_os_str()).collect();
                let mut prefix = PathBuf::new();
                for (left, right) in parts.iter().zip(other.iter()) {
                    if left == right {
                        prefix.push(left);
                    } else {
                        break;
                    }
                }
                prefix
            }
        });
    }
    let root = root.unwrap_or_else(|| PathBuf::from("."));
    if root == Path::new(".") {
        PathBuf::new()
    } else {
        root
    }
}

fn emitted_files(
    target: &str,
    ir: &IrProgram,
    with_tests: bool,
    with_usage: bool,
) -> Option<Vec<(PathBuf, String)>> {
    let mut files: Vec<(PathBuf, String)> = match target {
        "ts" => if with_tests {
            lt_emit_ts::emit_package_with_tests(ir)
        } else {
            lt_emit_ts::emit_package(ir)
        }
        .into_iter()
        .map(|file| (file.path, file.contents))
        .collect(),
        "py" => if with_tests {
            lt_emit_py::emit_package_with_tests(ir)
        } else {
            lt_emit_py::emit_package(ir)
        }
        .into_iter()
        .map(|file| (file.path, file.contents))
        .collect(),
        "go" => if with_tests {
            lt_emit_go::emit_package_with_tests(ir)
        } else {
            lt_emit_go::emit_package(ir)
        }
        .into_iter()
        .map(|file| (file.path, file.contents))
        .collect(),
        _ => return None,
    };
    if with_usage {
        let usage: Vec<(PathBuf, String)> = match target {
            "ts" => lt_emit_ts::emit_usage(ir)
                .into_iter()
                .map(|file| (file.path, file.contents))
                .collect(),
            "py" => lt_emit_py::emit_usage(ir)
                .into_iter()
                .map(|file| (file.path, file.contents))
                .collect(),
            _ => lt_emit_go::emit_usage(ir)
                .into_iter()
                .map(|file| (file.path, file.contents))
                .collect(),
        };
        files.extend(usage);
    }
    Some(files)
}

fn prepare(
    entries: &[PathBuf],
    mode: Mode,
) -> Result<(lt_hir::CheckOutput, lt_ir::IrProgram, Manager), i32> {
    let root = common_root(entries);

    let mut manager = Manager::new(&root, mode);
    let mut loader = FsLoader;
    let checked = check_entries_with_remote(entries, &mut loader, &mut manager);
    if let Err(error) = manager.save() {
        eprintln!("lt: {error}");
        return Err(1);
    }
    if !checked.is_ok() {
        print_file_diagnostics(&checked);
        return Err(1);
    }

    // One module per package: the output name is the source file's directory
    // plus the package name. Package names are unique per build, so modules
    // never collide.
    let mut names = Vec::new();
    for module in &checked.program.modules {
        let base = if module.origin.is_some() {
            module.path.clone()
        } else {
            match module.path.strip_prefix(&root) {
                Ok(relative) => relative.to_path_buf(),
                Err(_) => {
                    eprintln!(
                        "lt: `{}` is outside the entry directory `{}`",
                        module.path.display(),
                        root.display()
                    );
                    return Err(1);
                }
            }
        };
        let directory = base
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty());
        let name = match directory {
            Some(directory) => format!(
                "{}/{}",
                directory
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join("/"),
                module.name
            ),
            None => module.name.clone(),
        };
        names.push(name);
    }

    let (ir, lower_diagnostics) = lower_program(&checked.program, &checked.types, names);
    if !lower_diagnostics.is_empty() {
        for (module, diagnostic) in &lower_diagnostics {
            let path = &checked.program.modules[*module].path;
            let source = &checked.program.modules[*module].source;
            eprintln!("{}", diagnostic.render(&path.display().to_string(), source));
        }
        return Err(1);
    }

    Ok((checked, ir, manager))
}

fn cmd_ir(args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        eprintln!("lt: `ir` needs an entry file");
        return 2;
    };
    match prepare(&[PathBuf::from(path)], Mode::Locked) {
        Ok((_, ir, _)) => {
            println!("{ir:#?}");
            0
        }
        Err(code) => code,
    }
}

fn cmd_test(args: &[String]) -> i32 {
    let mut target = "ts".to_string();
    let mut out_dir = PathBuf::from("out");
    let mut run = false;
    let mut usage = false;
    let mut entries: Vec<PathBuf> = Vec::new();

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-t" | "--target" => {
                index += 1;
                match args.get(index) {
                    Some(value) => target = value.clone(),
                    None => {
                        eprintln!("lt: `-t` needs a target name");
                        return 2;
                    }
                }
            }
            "--out" => {
                index += 1;
                match args.get(index) {
                    Some(value) => out_dir = PathBuf::from(value),
                    None => {
                        eprintln!("lt: `--out` needs a directory");
                        return 2;
                    }
                }
            }
            "--run" => run = true,
            "--usage" => usage = true,
            flag if flag.starts_with('-') => {
                eprintln!("lt: unknown flag `{flag}`");
                return 2;
            }
            path => entries.push(PathBuf::from(path)),
        }
        index += 1;
    }

    if !matches!(target.as_str(), "ts" | "py" | "go") {
        eprintln!("lt: unknown target `{target}`; available: ts, py, go");
        return 2;
    }
    if entries.is_empty() {
        eprintln!("lt: `test` needs at least one entry file");
        return 2;
    }

    let (_checked, ir, _manager) = match prepare(&entries, Mode::Locked) {
        Ok(prepared) => prepared,
        Err(code) => return code,
    };

    let out_dir = out_dir.join(&target);
    let Some(files) = emitted_files(&target, &ir, true, usage) else {
        eprintln!("lt: unknown target `{target}`");
        return 2;
    };
    for (relative, contents) in &files {
        let path = out_dir.join(relative);
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                eprintln!("lt: {}: {error}", parent.display());
                return 1;
            }
        }
        if let Err(error) = std::fs::write(&path, contents) {
            eprintln!("lt: {}: {error}", path.display());
            return 1;
        }
    }
    println!("emitted tests to {}", out_dir.display());

    if !run {
        let hint = match target.as_str() {
            "ts" => format!("cd {} && npm install && npm test", out_dir.display()),
            "py" => format!("cd {} && pytest", out_dir.display()),
            _ => format!("cd {} && go test ./...", out_dir.display()),
        };
        println!("run with: {hint}");
        return 0;
    }

    let status = if target == "go" {
        std::process::Command::new("go")
            .arg("test")
            .arg("./...")
            .current_dir(&out_dir)
            .status()
    } else if target == "ts" {
        let local = out_dir.join("node_modules/.bin/vitest");
        if local.is_file() {
            std::process::Command::new(&local)
                .arg("run")
                .current_dir(&out_dir)
                .status()
        } else {
            std::process::Command::new("npx")
                .arg("--no-install")
                .arg("vitest")
                .arg("run")
                .current_dir(&out_dir)
                .status()
        }
    } else {
        std::process::Command::new("python3")
            .arg("-m")
            .arg("pytest")
            .arg("-q")
            .current_dir(&out_dir)
            .status()
    };
    match status {
        Ok(status) if status.success() => 0,
        Ok(_) => 1,
        Err(error) => {
            eprintln!("lt: cannot run the test runner ({error})");
            1
        }
    }
}

fn cmd_build(args: &[String]) -> i32 {
    let mut target = "ts".to_string();
    let mut out_dir = PathBuf::from("out");
    let mut entry: Option<PathBuf> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-t" | "--target" => {
                index += 1;
                match args.get(index) {
                    Some(value) => target = value.clone(),
                    None => {
                        eprintln!("lt: `-t` needs a target name");
                        return 2;
                    }
                }
            }
            "--out" => {
                index += 1;
                match args.get(index) {
                    Some(value) => out_dir = PathBuf::from(value),
                    None => {
                        eprintln!("lt: `--out` needs a directory");
                        return 2;
                    }
                }
            }
            flag if flag.starts_with('-') => {
                eprintln!("lt: unknown flag `{flag}`");
                return 2;
            }
            path => {
                if entry.is_some() {
                    eprintln!("lt: `build` takes exactly one entry file");
                    return 2;
                }
                entry = Some(PathBuf::from(path));
            }
        }
        index += 1;
    }

    if !matches!(target.as_str(), "ts" | "py" | "go") {
        eprintln!("lt: unknown target `{target}`; available: ts, py, go");
        return 2;
    }

    let Some(entry) = entry else {
        eprintln!("lt: `build` needs an entry file");
        return 2;
    };

    let (checked, ir, _manager) = match prepare(&[entry], Mode::Locked) {
        Ok(prepared) => prepared,
        Err(code) => return code,
    };

    let out_dir = out_dir.join(&target);
    let Some(files) = emitted_files(&target, &ir, false, false) else {
        eprintln!("lt: unknown target `{target}`");
        return 2;
    };
    for (relative, contents) in &files {
        let path = out_dir.join(relative);
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                eprintln!("lt: {}: {error}", parent.display());
                return 1;
            }
        }
        if let Err(error) = std::fs::write(&path, contents) {
            eprintln!("lt: {}: {error}", path.display());
            return 1;
        }
    }

    println!(
        "built {} module(s) to {}",
        checked.program.modules.len(),
        out_dir.display()
    );
    0
}

fn print_file_diagnostics(checked: &lt_hir::CheckOutput) {
    let sources: HashMap<PathBuf, (String, &str)> = checked
        .program
        .modules
        .iter()
        .map(|module| {
            let display = module
                .origin
                .clone()
                .unwrap_or_else(|| module.path.display().to_string());
            (module.path.clone(), (display, module.source.as_str()))
        })
        .collect();
    for file in &checked.diagnostics {
        let name = file.path.display().to_string();
        match sources.get(&file.path) {
            Some((display, source)) => eprintln!("{}", file.diagnostic.render(display, source)),
            None => eprintln!("error: {}: {}", name, file.diagnostic.message),
        }
    }
}

fn parse_entries(args: &[String], command: &str) -> Result<Vec<PathBuf>, i32> {
    let mut entries = Vec::new();
    for arg in args {
        if arg.starts_with('-') {
            eprintln!("lt: unknown flag `{arg}`");
            return Err(2);
        }
        entries.push(PathBuf::from(arg));
    }
    if entries.is_empty() {
        eprintln!("lt: `{command}` needs at least one entry file");
        return Err(2);
    }
    Ok(entries)
}

fn cmd_update(args: &[String]) -> i32 {
    let entries = match parse_entries(args, "update") {
        Ok(entries) => entries,
        Err(code) => return code,
    };
    match prepare(&entries, Mode::Update) {
        Ok((checked, _ir, manager)) => {
            println!(
                "updated lt.lock ({} package(s), {} fetched, {} module(s))",
                manager.lock.packages.len(),
                manager.fetched.len(),
                checked.program.modules.len()
            );
            0
        }
        Err(code) => code,
    }
}

fn cmd_vendor(args: &[String]) -> i32 {
    let entries = match parse_entries(args, "vendor") {
        Ok(entries) => entries,
        Err(code) => return code,
    };
    match prepare(&entries, Mode::Locked) {
        Ok((_checked, _ir, mut manager)) => match manager.vendor() {
            Ok(count) => {
                println!(
                    "vendored {count} package(s) into {}",
                    manager.vendor_dir.display()
                );
                0
            }
            Err(error) => {
                eprintln!("lt: {error}");
                1
            }
        },
        Err(code) => code,
    }
}

fn cmd_playground(args: &[String]) -> i32 {
    let mut root = PathBuf::from("playground");
    let mut out = PathBuf::from("out/playground");
    let mut allow_stale = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                match args.get(index) {
                    Some(value) => root = PathBuf::from(value),
                    None => {
                        eprintln!("lt: `--root` needs a directory");
                        return 2;
                    }
                }
            }
            "--out" => {
                index += 1;
                match args.get(index) {
                    Some(value) => out = PathBuf::from(value),
                    None => {
                        eprintln!("lt: `--out` needs a directory");
                        return 2;
                    }
                }
            }
            "--allow-stale" => allow_stale = true,
            flag => {
                eprintln!("lt: unknown flag `{flag}`");
                return 2;
            }
        }
        index += 1;
    }

    match lt_playground::generate(&lt_playground::Options {
        root,
        out,
        allow_stale,
    }) {
        Ok(summary) => {
            println!(
                "playground: {} cases ({} diagnostics), {} features -> {}",
                summary.cases,
                summary.diagnostics,
                summary.features,
                summary.output.display()
            );
            if summary.stale_evidence {
                eprintln!("lt: evidence.json is stale; run scripts/playground.sh");
            }
            0
        }
        Err(error) => {
            eprintln!("lt: {error}");
            1
        }
    }
}

fn cmd_ast(inputs: &[String]) -> i32 {
    if inputs.is_empty() {
        eprintln!("lt: expected a path");
        return 2;
    }
    let mut failed = false;
    for input in inputs {
        let path = PathBuf::from(input);
        let src = match read_source(&path) {
            Ok(src) => src,
            Err(code) => return code,
        };
        let out = parse(&src);
        if !out.diagnostics.is_empty() {
            failed = true;
            report(&path, &src, &out.diagnostics);
            continue;
        }
        println!("{:#?}", out.file);
    }
    if failed {
        1
    } else {
        0
    }
}

fn cmd_fmt(args: &[String]) -> i32 {
    let mut check = false;
    let mut inputs: Vec<String> = Vec::new();
    for arg in args {
        if arg == "--check" {
            check = true;
        } else if arg.starts_with('-') {
            eprintln!("lt: unknown flag `{arg}`");
            return 2;
        } else {
            inputs.push(arg.clone());
        }
    }
    if inputs.is_empty() {
        eprintln!("lt: expected at least one path");
        return 2;
    }

    let paths = match collect_paths(&inputs) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("lt: {error}");
            return 2;
        }
    };

    let mut failed = false;
    for path in &paths {
        let src = match read_source(path) {
            Ok(src) => src,
            Err(code) => return code,
        };

        let lexed = tokenize_full(&src);
        if !lexed.errors.is_empty() {
            failed = true;
            report(path, &src, &lexed.errors);
            continue;
        }

        let out = parse(&src);
        if !out.diagnostics.is_empty() {
            failed = true;
            report(path, &src, &out.diagnostics);
            continue;
        }

        let formatted = match format_file_with_comments(&out.file, &src, &lexed.comments) {
            Ok(text) => text,
            Err(error) => {
                eprintln!(
                    "error: {}\n  --> {}:{}:{}",
                    error.message,
                    path.display(),
                    error.line,
                    error.col
                );
                failed = true;
                continue;
            }
        };
        if check {
            if src != formatted {
                eprintln!("would reformat {}", path.display());
                failed = true;
            }
        } else if src != formatted {
            if let Err(error) = std::fs::write(path, &formatted) {
                eprintln!("lt: {}: {error}", path.display());
                failed = true;
            } else {
                println!("formatted {}", path.display());
            }
        }
    }

    if failed {
        1
    } else {
        0
    }
}
