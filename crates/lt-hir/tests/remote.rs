use std::collections::HashMap;
use std::path::PathBuf;

use lt_hir::{check_entries_with_remote, join_url, MapLoader, RemoteResolver, ResolvedRemote};
use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct FakeRemote {
    files: HashMap<String, String>,
}

impl FakeRemote {
    fn new(files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(url, body)| (url.to_string(), body.to_string()))
                .collect(),
        }
    }
}

impl RemoteResolver for FakeRemote {
    fn resolve(&mut self, url: &str) -> Result<ResolvedRemote, String> {
        let body = self
            .files
            .get(url)
            .cloned()
            .ok_or_else(|| format!("unexpected url `{url}`"))?;
        Ok(ResolvedRemote {
            canonical_url: url.to_string(),
            sha256: sha256_hex(body.as_bytes()),
            content: body,
        })
    }
}

fn check(files: &[(&str, &str)], remote: &[(&str, &str)], entry: &str) -> lt_hir::CheckOutput {
    let mut loader = MapLoader::new();
    for (path, source) in files {
        loader.add(path, source);
    }
    let mut resolver = FakeRemote::new(remote);
    check_entries_with_remote(&[PathBuf::from(entry)], &mut loader, &mut resolver)
}

const REMOTE_USER: &str = r#"
import helper { twice } "./helper.lt"

package remote_user {

export struct RemoteUser {
  name: Str
}

export fn doubled(u: RemoteUser, x: Int) -> Int {
  twice(x)
}
}
"#;

const REMOTE_HELPER: &str = r#"
package helper {

export fn twice(x: Int) -> Int { x * 2 }
}
"#;

#[test]
fn remote_imports_resolve_relative_to_their_url() {
    let output = check(
        &[(
            "main.lt",
            "import remote_user { RemoteUser } \"http://example.test/pkg/user.lt\"\n\npackage app {\n\nexport struct App {\n  user: RemoteUser\n}\n}\n",
        )],
        &[
            ("http://example.test/pkg/user.lt", REMOTE_USER),
            ("http://example.test/pkg/helper.lt", REMOTE_HELPER),
        ],
        "main.lt",
    );
    assert!(output.is_ok(), "{:#?}", output.diagnostics);
    assert_eq!(output.program.modules.len(), 3);

    let remote: Vec<_> = output
        .program
        .modules
        .iter()
        .filter(|module| module.origin.is_some())
        .collect();
    assert_eq!(remote.len(), 2);
    assert!(remote
        .iter()
        .any(|module| module.origin.as_deref() == Some("http://example.test/pkg/helper.lt")));
    assert!(remote.iter().all(|module| module
        .path
        .to_string_lossy()
        .starts_with("remote/example_test/")));
}

#[test]
fn remote_cycle_is_reported() {
    let output = check(
        &[(
            "main.lt",
            "import a { A } \"http://example.test/a.lt\"\n\npackage app {\n\nexport struct App {\n  holder: A\n}\n}\n",
        )],
        &[
            (
                "http://example.test/a.lt",
                "import b { B } \"./b.lt\"\n\npackage a {\n\nexport struct A { x: Int }\n}\n",
            ),
            (
                "http://example.test/b.lt",
                "import a { A } \"./a.lt\"\n\npackage b {\n\nexport struct B { y: Int }\n}\n",
            ),
        ],
        "main.lt",
    );
    assert!(
        output
            .diagnostics
            .iter()
            .any(|file| file.diagnostic.message.contains("import cycle")),
        "{:#?}",
        output.diagnostics
    );
}

#[test]
fn url_join_normalizes_relative_segments() {
    assert_eq!(
        join_url(
            "https://raw.githubusercontent.com/o/r/sha/src/user.lt",
            "./helper.lt"
        ),
        "https://raw.githubusercontent.com/o/r/sha/src/helper.lt"
    );
    assert_eq!(
        join_url("https://example.test/a/b/user.lt", "../shared/x.lt"),
        "https://example.test/a/shared/x.lt"
    );
    assert_eq!(
        join_url("https://example.test/a/user.lt", "https://other.test/x.lt"),
        "https://other.test/x.lt"
    );
}

#[test]
fn remote_missing_reports_the_url() {
    let output = check(
        &[(
            "main.lt",
            "import a { A } \"http://example.test/missing.lt\"\n\npackage app {\n  a: A\n}\n",
        )],
        &[],
        "main.lt",
    );
    assert!(!output.is_ok());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|file| file.diagnostic.message.contains("unexpected url")),
        "{:#?}",
        output.diagnostics
    );
}
