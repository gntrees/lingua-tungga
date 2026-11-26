//! Python backend for Lingua Tungga.

pub mod py;

use std::collections::BTreeSet;
use std::path::PathBuf;

use lt_ir::IrProgram;

pub const RUNTIME: &str = include_str!("../../../runtimes/py/runtime.py");

#[derive(Debug, Clone)]
pub struct EmittedFile {
    pub path: PathBuf,
    pub contents: String,
}

pub fn emit_package(program: &IrProgram) -> Vec<EmittedFile> {
    let mut files = vec![
        EmittedFile {
            path: PathBuf::from("lt_runtime.py"),
            contents: RUNTIME.to_string(),
        },
        EmittedFile {
            path: PathBuf::from("pyproject.toml"),
            contents: pyproject(),
        },
        EmittedFile {
            path: PathBuf::from("gen/__init__.py"),
            contents: String::new(),
        },
    ];

    let mut directories: BTreeSet<String> = BTreeSet::new();
    for module in &program.modules {
        let mut prefix = String::from("gen");
        let parts: Vec<&str> = module.name.split('/').collect();
        for part in &parts[..parts.len().saturating_sub(1)] {
            prefix.push('/');
            prefix.push_str(part);
            directories.insert(prefix.clone());
        }
        files.push(EmittedFile {
            path: PathBuf::from(format!("gen/{}.py", module.name)),
            contents: py::emit_module(program, module),
        });
    }
    for directory in directories {
        files.push(EmittedFile {
            path: PathBuf::from(format!("{directory}/__init__.py")),
            contents: String::new(),
        });
    }

    files
}

pub fn emit_tests(program: &IrProgram) -> Vec<EmittedFile> {
    let mut files = Vec::new();
    for module in &program.modules {
        if let Some(contents) = py::emit_tests(program, module) {
            files.push(EmittedFile {
                path: PathBuf::from(format!(
                    "tests/test_{}.py",
                    module.name.replace('/', "_")
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

fn pyproject() -> String {
    "[project]\nname = \"lingua-tungga-generated\"\nversion = \"0.0.0\"\nrequires-python = \">=3.11\"\n\n[tool.pytest.ini_options]\npythonpath = [\".\"]\ntestpaths = [\"tests\"]\n".to_string()
}
