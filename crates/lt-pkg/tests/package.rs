use std::collections::HashMap;
use std::path::PathBuf;

use lt_hir::RemoteResolver;
use lt_pkg::{sha256_hex, HttpFetcher, Lockfile, Manager, Mode};

struct FakeFetcher {
    responses: HashMap<String, Vec<u8>>,
    calls: usize,
}

impl FakeFetcher {
    fn new() -> Self {
        Self {
            responses: HashMap::new(),
            calls: 0,
        }
    }

    fn with(mut self, url: &str, body: &str) -> Self {
        self.responses
            .insert(url.to_string(), body.as_bytes().to_vec());
        self
    }
}

impl HttpFetcher for FakeFetcher {
    fn get(&mut self, url: &str) -> Result<Vec<u8>, String> {
        self.calls += 1;
        self.responses
            .get(url)
            .cloned()
            .ok_or_else(|| format!("unexpected fetch of `{url}`"))
    }
}

fn temp_project(tag: &str) -> PathBuf {
    let unique = format!(
        "lt-pkg-{tag}-{}-{}",
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

fn manager(
    root: &std::path::Path,
    mode: Mode,
    cache: &std::path::Path,
    fetcher: FakeFetcher,
) -> Manager {
    let mut manager = Manager::with_fetcher(root, mode, Box::new(fetcher));
    manager.cache_dir = cache.to_path_buf();
    manager
}

#[test]
fn lockfile_round_trips() {
    let dir = temp_project("lock");
    let path = dir.join("lt.lock");
    let text = "# generated\n\n[[package]]\nurl = \"http://x/a.lt\"\ncanonical = \"http://x/a.lt\"\ncommit = \"abc\"\nsha256 = \"def\"\n";
    let lock = Lockfile::parse(text).unwrap();
    assert_eq!(lock.packages.len(), 1);
    let package = lock.packages.get("http://x/a.lt").unwrap();
    assert_eq!(package.commit.as_deref(), Some("abc"));
    lock.save(&path).unwrap();
    let reloaded = Lockfile::load(&path).unwrap();
    assert_eq!(
        reloaded.packages.get("http://x/a.lt").unwrap().sha256,
        "def"
    );
}

#[test]
fn fresh_resolve_writes_lock_and_cache() {
    let root = temp_project("fresh");
    let cache = root.join("cache");
    let body = "pub class A { x: Int }\n";
    let url = "http://example.test/a.lt";
    let mut manager = manager(
        &root,
        Mode::Locked,
        &cache,
        FakeFetcher::new().with(url, body),
    );

    let resolved = manager.resolve(url).unwrap();
    assert_eq!(resolved.content, body);
    assert_eq!(resolved.sha256, sha256_hex(body.as_bytes()));
    manager.save().unwrap();

    let lock = Lockfile::load(&root.join("lt.lock")).unwrap();
    assert_eq!(lock.packages.get(url).unwrap().sha256, resolved.sha256);
    assert!(cache.join("objects").join(&resolved.sha256).is_file());
}

#[test]
fn locked_resolve_uses_cache_without_network() {
    let root = temp_project("cache");
    let cache = root.join("cache");
    let body = "pub class A { x: Int }\n";
    let url = "http://example.test/a.lt";

    let mut first = manager(
        &root,
        Mode::Locked,
        &cache,
        FakeFetcher::new().with(url, body),
    );
    let resolved = first.resolve(url).unwrap();
    first.save().unwrap();

    // Second manager has no responses: any fetch is an error.
    let mut offline = manager(&root, Mode::Locked, &cache, FakeFetcher::new());
    let again = offline.resolve(url).unwrap();
    assert_eq!(again.sha256, resolved.sha256);
}

#[test]
fn tampered_content_is_rejected() {
    let root = temp_project("mismatch");
    let cache = root.join("cache");
    let url = "http://example.test/a.lt";

    let mut first = manager(
        &root,
        Mode::Locked,
        &cache,
        FakeFetcher::new().with(url, "one\n"),
    );
    first.resolve(url).unwrap();
    first.save().unwrap();

    std::fs::remove_dir_all(&cache).unwrap();
    let mut second = manager(
        &root,
        Mode::Locked,
        &cache,
        FakeFetcher::new().with(url, "two\n"),
    );
    let error = second.resolve(url).unwrap_err();
    assert!(error.contains("checksum mismatch"), "{error}");
    assert!(error.contains("lt update"), "{error}");
}

#[test]
fn update_mode_accepts_new_content() {
    let root = temp_project("update");
    let cache = root.join("cache");
    let url = "http://example.test/a.lt";

    let mut first = manager(
        &root,
        Mode::Locked,
        &cache,
        FakeFetcher::new().with(url, "one\n"),
    );
    first.resolve(url).unwrap();
    first.save().unwrap();

    let mut update = manager(
        &root,
        Mode::Update,
        &cache,
        FakeFetcher::new().with(url, "two\n"),
    );
    let resolved = update.resolve(url).unwrap();
    update.save().unwrap();

    let lock = Lockfile::load(&root.join("lt.lock")).unwrap();
    let package = lock.packages.get(url).unwrap();
    assert_eq!(package.sha256, resolved.sha256);
    assert_eq!(package.sha256, sha256_hex(b"two\n"));
}

#[test]
fn vendor_allows_offline_resolution() {
    let root = temp_project("vendor");
    let cache = root.join("cache");
    let body = "pub class A { x: Int }\n";
    let url = "http://example.test/a.lt";

    let mut first = manager(
        &root,
        Mode::Locked,
        &cache,
        FakeFetcher::new().with(url, body),
    );
    let resolved = first.resolve(url).unwrap();
    first.save().unwrap();

    let vendored = first.vendor().unwrap();
    assert_eq!(vendored, 1);
    assert!(root.join("vendor").join(&resolved.sha256).is_file());

    std::fs::remove_dir_all(&cache).unwrap();
    let mut offline = manager(&root, Mode::Locked, &cache, FakeFetcher::new());
    let again = offline.resolve(url).unwrap();
    assert_eq!(again.content, body);
}

#[test]
fn raw_github_urls_pin_to_a_commit() {
    let root = temp_project("github");
    let cache = root.join("cache");
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let url = "https://raw.githubusercontent.com/foo/bar/main/src/a.lt";
    let api = "https://api.github.com/repos/foo/bar/commits/main";
    let pinned = format!("https://raw.githubusercontent.com/foo/bar/{sha}/src/a.lt");

    let fetcher = FakeFetcher::new()
        .with(api, &format!("{{\"sha\":\"{sha}\",\"commit\":{{}}}}"))
        .with(&pinned, "pub class A { x: Int }\n");
    let mut manager = manager(&root, Mode::Locked, &cache, fetcher);
    let resolved = manager.resolve(url).unwrap();
    assert_eq!(resolved.canonical_url, pinned);
    manager.save().unwrap();

    let lock = Lockfile::load(&root.join("lt.lock")).unwrap();
    let package = lock.packages.get(url).unwrap();
    assert_eq!(package.commit.as_deref(), Some(sha));
    assert_eq!(package.canonical, pinned);
}
