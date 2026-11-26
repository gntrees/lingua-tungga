//! `lt` — Lingua Tungga command line interface.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::exit;

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
  lt check <path>...        parse files and report diagnostics
  lt fmt [--check] <path>   format files in place (or check formatting)
  lt ast <path>             parse and dump the AST (debugging)

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

    let mut failed = false;
    for path in &paths {
        let src = match read_source(path) {
            Ok(src) => src,
            Err(code) => return code,
        };
        let out = parse(&src);
        if out.diagnostics.is_empty() {
            println!("ok  {}", path.display());
        } else {
            failed = true;
            report(path, &src, &out.diagnostics);
        }
    }

    if failed {
        1
    } else {
        0
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
