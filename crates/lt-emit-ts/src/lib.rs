//! TypeScript backend for Lingua Tungga.

pub mod ts;

use std::path::PathBuf;

use lt_ir::IrProgram;

/// The runtime is inlined into every generated package.
pub const RUNTIME: &str = include_str!("../../../runtimes/ts/runtime.ts");

#[derive(Debug, Clone)]
pub struct EmittedFile {
    /// Path relative to the output package root.
    pub path: PathBuf,
    pub contents: String,
}

/// Emit only the library package (no tests).
pub fn emit_package(program: &IrProgram) -> Vec<EmittedFile> {
    let mut files = vec![
        EmittedFile {
            path: PathBuf::from("_lingua_rt.ts"),
            contents: RUNTIME.to_string(),
        },
        EmittedFile {
            path: PathBuf::from("package.json"),
            contents: package_json(),
        },
        EmittedFile {
            path: PathBuf::from("tsconfig.json"),
            contents: tsconfig_json(),
        },
    ];

    for module in &program.modules {
        files.push(EmittedFile {
            path: PathBuf::from(format!("src/{}.ts", module.name)),
            contents: ts::emit_module(program, module),
        });
    }

    files
}

/// Emit the `vitest` files for every module that declares test classes.
pub fn emit_tests(program: &IrProgram) -> Vec<EmittedFile> {
    let mut files = Vec::new();
    for module in &program.modules {
        if let Some(contents) = ts::emit_tests(program, module) {
            files.push(EmittedFile {
                path: PathBuf::from(format!("tests/{}.test.ts", module.name)),
                contents,
            });
        }
    }
    files
}

/// Library package plus generated tests.
pub fn emit_package_with_tests(program: &IrProgram) -> Vec<EmittedFile> {
    let mut files = emit_package(program);
    files.extend(emit_tests(program));
    files
}

/// One runnable program per module: how to call this library from TypeScript.
pub fn emit_usage(program: &IrProgram) -> Vec<EmittedFile> {
    let mut files = Vec::new();
    for module in &program.modules {
        if let Some(contents) = ts::emit_usage(program, module) {
            files.push(EmittedFile {
                path: PathBuf::from(format!("src/{}.usage.ts", module.name)),
                contents,
            });
        }
    }
    files
}

fn package_json() -> String {
    "{\n  \"name\": \"lingua-tungga-generated\",\n  \"private\": true,\n  \"type\": \"module\",\n  \"scripts\": {\n    \"test\": \"vitest run\"\n  },\n  \"devDependencies\": {\n    \"typescript\": \"^5.4.0\",\n    \"vitest\": \"^3.0.0\"\n  }\n}\n".to_string()
}

fn tsconfig_json() -> String {
    "{\n  \"compilerOptions\": {\n    \"target\": \"ES2022\",\n    \"module\": \"ESNext\",\n    \"moduleResolution\": \"bundler\",\n    \"strict\": true,\n    \"skipLibCheck\": true,\n    \"noEmit\": true\n  },\n  \"include\": [\"_lingua_rt.ts\", \"src/**/*.ts\", \"tests/**/*.ts\"]\n}\n".to_string()
}
