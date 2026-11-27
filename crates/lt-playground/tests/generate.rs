use std::path::{Path, PathBuf};

use lt_playground::{generate, Options};

fn temp_root(tag: &str) -> PathBuf {
    let unique = format!(
        "lt-playground-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, path: &str, contents: &str) {
    let full = root.join(path);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(full, contents).unwrap();
}

fn fixture(root: &Path) {
    write(
        root,
        "features.toml",
        r##"
[[feature]]
id = "demo/run"
title = "Kasus bisa dijalankan"
spec = "#1"

[[feature]]
id = "demo/diag"
title = "Kasus error"
spec = "#15"
"##,
    );
    write(
        root,
        "cases/demo/ok.lt",
        "package ok {\n\n  test \"passes\" {\n    expect.eq(1, 1)\n  }\n\n}\n",
    );
    write(
        root,
        "cases/demo/ok.toml",
        "title = \"Kasus ok\"\nfeature = \"demo/run\"\nwhy = \"karena\"\n",
    );
    write(
        root,
        "diagnostics/bad.lt",
        "package bad {\n\n  export struct A {\n    field: Missing\n  }\n\n}\n",
    );
    write(
        root,
        "diagnostics/bad.toml",
        "title = \"Kasus gagal\"\nfeature = \"demo/diag\"\nwhy = \"harus gagal\"\nexpect = \"unknown type\"\n",
    );
}

fn options(root: &Path, allow_stale: bool) -> Options {
    Options {
        root: root.to_path_buf(),
        out: root.join("out"),
        allow_stale,
    }
}

#[test]
fn generates_a_page_from_cases() {
    let root = temp_root("ok");
    fixture(&root);
    let summary = generate(&options(&root, true)).unwrap();
    assert_eq!(summary.cases, 1);
    assert_eq!(summary.diagnostics, 1);
    assert_eq!(summary.features, 2);
    assert!(!summary.stale_evidence);

    let html = std::fs::read_to_string(root.join("out/index.html")).unwrap();
    assert!(html.contains("Kasus ok"), "case missing from page");
    assert!(html.contains("Kasus gagal"), "diagnostic missing from page");
    assert!(html.contains("unknown type"), "diagnostic message missing");
    assert!(html.contains("const DATA ="), "data missing");
}

#[test]
fn refuses_features_without_cases() {
    let root = temp_root("coverage");
    fixture(&root);
    write(
        &root,
        "features.toml",
        "[[feature]]\nid = \"demo/run\"\ntitle = \"ada\"\n\n[[feature]]\nid = \"demo/diag\"\ntitle = \"ada juga\"\n\n[[feature]]\nid = \"demo/missing\"\ntitle = \"tidak ada\"\n",
    );
    let error = generate(&options(&root, true)).unwrap_err();
    assert!(error.contains("without a case"), "{error}");
    assert!(error.contains("demo/missing"), "{error}");
}

#[test]
fn stale_evidence_is_rejected_unless_allowed() {
    let root = temp_root("stale");
    fixture(&root);
    write(
        &root,
        "evidence.json",
        r#"{"schema":1,"generated_at":"now","targets":{},"cases":{"other/case":{"ts":true,"py":true,"go":true}}}"#,
    );
    let error = generate(&options(&root, false)).unwrap_err();
    assert!(error.contains("does not match"), "{error}");
    assert!(error.contains("scripts/playground.sh"), "{error}");

    let summary = generate(&options(&root, true)).unwrap();
    assert!(summary.stale_evidence);
    let html = std::fs::read_to_string(root.join("out/index.html")).unwrap();
    assert!(html.contains("\"stale\":true"), "stale flag missing");
}

#[test]
fn wrong_diagnostic_expectation_fails_generation() {
    let root = temp_root("diag");
    fixture(&root);
    write(
        &root,
        "diagnostics/bad.toml",
        "title = \"Kasus gagal\"\nfeature = \"demo/diag\"\nwhy = \"harus gagal\"\nexpect = \"pesan yang tidak ada\"\n",
    );
    let error = generate(&options(&root, true)).unwrap_err();
    assert!(error.contains("no diagnostic contains"), "{error}");
}
