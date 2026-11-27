//! Go backend for Lingua Tungga.

pub mod go;

use std::path::PathBuf;

use lt_ir::IrProgram;

pub const RUNTIME: &str = include_str!("../../../runtimes/go/runtime.go");

#[derive(Debug, Clone)]
pub struct EmittedFile {
    pub path: PathBuf,
    pub contents: String,
}

fn module_dir(name: &str) -> String {
    format!("gen/{name}")
}

fn module_base(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

pub fn emit_package(program: &IrProgram) -> Vec<EmittedFile> {
    let mut files = vec![
        EmittedFile {
            path: PathBuf::from("go.mod"),
            contents: "module linguatungga\n\ngo 1.22\n".to_string(),
        },
        EmittedFile {
            path: PathBuf::from("lt/runtime.go"),
            contents: RUNTIME.to_string(),
        },
    ];

    for module in &program.modules {
        files.push(EmittedFile {
            path: PathBuf::from(format!(
                "{}/{}.go",
                module_dir(&module.name),
                module_base(&module.name)
            )),
            contents: go::emit_module(program, module),
        });
    }

    files
}

pub fn emit_tests(program: &IrProgram) -> Vec<EmittedFile> {
    let mut files = Vec::new();
    for module in &program.modules {
        if let Some(contents) = go::emit_tests(program, module) {
            files.push(EmittedFile {
                path: PathBuf::from(format!(
                    "{}/{}_test.go",
                    module_dir(&module.name),
                    module_base(&module.name)
                )),
                contents,
            });
        }
    }
    files
}

pub fn emit_package_with_tests(program: &IrProgram) -> Vec<EmittedFile> {
    let mut files = emit_package(program);
    files.extend(emit_tests(program));
    files
}

/// One runnable `main` package per module: how to call it from Go.
pub fn emit_usage(program: &IrProgram) -> Vec<EmittedFile> {
    let mut files = Vec::new();
    for module in &program.modules {
        if let Some(contents) = go::emit_usage(program, module) {
            files.push(EmittedFile {
                path: PathBuf::from(format!("usage/{}/main.go", module.name)),
                contents,
            });
        }
    }
    files
}
