use std::path::PathBuf;

use lt_emit_ts::{emit_package, emit_tests};
use lt_hir::{check_entries, MapLoader};
use lt_ir::lower_program;

const PROGRAM: &str = r#"
pub enum Role {
  Admin
  Member(Int)
}

pub class User {
  name: Str
  role: Role

  pub fn label(self) -> Str {
    match self.role {
      Role.Admin => "admin"
      Role.Member(level) => `member-${level}`
    }
  }
}

test class UserTest {
  test fn label_of_admin() {
    Expect.equal(User(name: "a", role: Role.Admin).label(), "admin")
  }
}
"#;

fn emit() -> Vec<(PathBuf, String)> {
    let mut loader = MapLoader::new();
    loader.add("user.lt", PROGRAM);
    let checked = check_entries(&[PathBuf::from("user.lt")], &mut loader);
    assert!(checked.is_ok(), "{:#?}", checked.diagnostics);

    let names = checked
        .program
        .modules
        .iter()
        .map(|module| {
            module
                .path
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();

    let (ir, diagnostics) = lower_program(&checked.program, &checked.types, names);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");

    let mut files: Vec<(PathBuf, String)> = emit_package(&ir)
        .into_iter()
        .filter(|file| file.path != std::path::Path::new("_lingua_rt.ts"))
        .map(|file| (file.path, file.contents))
        .collect();
    files.extend(
        emit_tests(&ir)
            .into_iter()
            .map(|file| (file.path, file.contents)),
    );
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

#[test]
fn emitter_output_matches_golden_files() {
    let update = std::env::var("UPDATE_GOLDEN").is_ok();
    let files = emit();

    if update {
        std::fs::create_dir_all(golden_dir()).unwrap();
    }

    for (path, contents) in &files {
        let golden = golden_dir().join(path);
        if update {
            if let Some(parent) = golden.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&golden, contents).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&golden).unwrap_or_else(|_| {
            panic!(
                "missing golden file {}; run `UPDATE_GOLDEN=1 cargo test -p lt-emit-ts`",
                golden.display()
            )
        });
        assert_eq!(
            contents, &expected,
            "emitted file {} differs from its golden copy",
            path.display()
        );
    }

    if !update {
        // No stale golden files.
        let mut expected_paths: Vec<PathBuf> = files.iter().map(|(path, _)| path.clone()).collect();
        expected_paths.sort();
        let mut found: Vec<PathBuf> = walk(&golden_dir());
        found.sort();
        assert_eq!(found, expected_paths, "golden directory has extra files");
    }
}

fn walk(root: &std::path::Path) -> Vec<PathBuf> {
    fn collect(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(root, &path, out);
            } else if let Ok(relative) = path.strip_prefix(root) {
                out.push(relative.to_path_buf());
            }
        }
    }
    let mut out = Vec::new();
    collect(root, root, &mut out);
    out
}

#[test]
fn emission_is_deterministic() {
    let first = emit();
    let second = emit();
    assert_eq!(first, second);
}
