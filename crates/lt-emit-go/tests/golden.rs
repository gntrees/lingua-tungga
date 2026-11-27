use std::path::PathBuf;

use lt_emit_go::{emit_package, emit_tests};
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
        .filter(|file| file.path != std::path::Path::new("lt/runtime.go"))
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
fn go_emitter_matches_golden_files() {
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
                "missing golden file {}; run `UPDATE_GOLDEN=1 cargo test -p lt-emit-go`",
                golden.display()
            )
        });
        // A Windows checkout may turn golden files into CRLF; compare content.
        assert_eq!(
            contents,
            &expected.replace("\r\n", "\n"),
            "emitted {} differs",
            path.display()
        );
    }
}
