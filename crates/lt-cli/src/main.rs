//! `lt` — Lingua Tungga command line interface.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::exit;

use std::collections::HashMap;

use lt_hir::{check_entries, FsLoader};
use lt_ir::lower_program;
use lt_syntax::{format_file, parse, span::line_col_at, tokenize_full, Diagnostic};

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
        Some("ir") => cmd_ir(&args[1..]),
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
  lt build -t ts <entry>    compile to a TypeScript package (default out/ts)
  lt ir <entry>             lower to IR and dump it (debugging)

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

    let mut loader = FsLoader;
    let out = check_entries(&paths, &mut loader);

    if out.diagnostics.is_empty() {
        for path in &paths {
            println!("ok  {}", path.display());
        }
        return 0;
    }

    print_file_diagnostics(&out);
    1
}

fn prepare(entry: &Path) -> Result<(lt_hir::CheckOutput, lt_ir::IrProgram), i32> {
    let mut loader = FsLoader;
    let checked = check_entries(&[entry.to_path_buf()], &mut loader);
    if !checked.is_ok() {
        print_file_diagnostics(&checked);
        return Err(1);
    }

    let root = entry
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut names = Vec::new();
    for module in &checked.program.modules {
        let relative = match module.path.strip_prefix(&root) {
            Ok(relative) => relative,
            Err(_) => {
                eprintln!(
                    "lt: `{}` is outside the entry directory `{}`",
                    module.path.display(),
                    root.display()
                );
                return Err(1);
            }
        };
        let name = relative
            .with_extension("")
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
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

    Ok((checked, ir))
}

fn cmd_ir(args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        eprintln!("lt: `ir` needs an entry file");
        return 2;
    };
    match prepare(Path::new(path)) {
        Ok((_, ir)) => {
            println!("{ir:#?}");
            0
        }
        Err(code) => code,
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

    if target != "ts" {
        eprintln!("lt: unknown target `{target}`; available: ts");
        return 2;
    }

    let Some(entry) = entry else {
        eprintln!("lt: `build` needs an entry file");
        return 2;
    };

    let (checked, ir) = match prepare(&entry) {
        Ok(prepared) => prepared,
        Err(code) => return code,
    };

    let out_dir = out_dir.join(&target);
    let files = lt_emit_ts::emit_package(&ir);
    for file in &files {
        let path = out_dir.join(&file.path);
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                eprintln!("lt: {}: {error}", parent.display());
                return 1;
            }
        }
        if let Err(error) = std::fs::write(&path, &file.contents) {
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
    let sources: HashMap<PathBuf, &str> = checked
        .program
        .modules
        .iter()
        .map(|module| (module.path.clone(), module.source.as_str()))
        .collect();
    for file in &checked.diagnostics {
        let name = file.path.display().to_string();
        match sources.get(&file.path) {
            Some(source) => eprintln!("{}", file.diagnostic.render(&name, source)),
            None => eprintln!("error: {}: {}", name, file.diagnostic.message),
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

        // Comments are dropped by the token stream, so formatting would delete
        // them. Refuse instead of losing data silently.
        if let Some(span) = lexed.comments.first() {
            let loc = line_col_at(&src, span.start);
            eprintln!(
                "error: `lt fmt` cannot preserve comments yet; refusing to reformat so nothing is lost\n  --> {}:{}:{}",
                path.display(),
                loc.line,
                loc.col
            );
            failed = true;
            continue;
        }

        let formatted = format_file(&out.file);
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
