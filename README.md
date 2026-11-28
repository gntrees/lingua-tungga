# Lingua Tungga

Satu sumber untuk logika murni dan model data yang berperilaku **identik** di TypeScript, Python, dan Go.

```lt
package money

export struct Price {
  amount: Int
  currency: Str
}

export fn format(p: Price) -> Str {
  `${p.amount / 100}.${p.amount % 100} ${p.currency}`
}
```

Rilis dibangun lokal (`./scripts/release.sh v0.4.0 --publish`) — lihat
[docs/release.md](docs/release.md).

Pasang compiler (tanpa Rust, binary rilis + verifikasi checksum):

```sh
curl -fsSL https://raw.githubusercontent.com/gntrees/lingua-tungga/main/scripts/install.sh | bash
# atau kunci versi: LT_VERSION=v0.4.0 ... | bash
# dari sumber: cargo install --path crates/lt-cli
```

```sh
lt build -t go      # → out/go, siap di-import proyek Go
lt build -t py      # → out/py, siap di-import proyek Python
lt build -t ts      # → out/ts, siap di-import proyek TypeScript
lt test -t go       # jalankan conformance suite di target Go
```

Hari ini yang sudah bisa dipakai:

```sh
lt check src/                          # resolve import + typecheck seluruh graf
lt fmt src/                            # format kanonik (idempoten)
lt fmt --check src/                    # exit 1 kalau ada file belum terformat
lt ast src/a.lt                        # dump AST untuk debugging
lt ir src/a.lt                         # dump IR hasil lowering
lt build -t ts examples/validator.lt   # → out/ts, package TypeScript siap tsc
lt test  -t ts tests/conformance/*.lt --out out/conf --run   # conformance (vitest)
lt test  -t py tests/conformance/*.lt --out out/conf --run   # conformance (pytest)
lt test  -t go tests/conformance/*.lt --out out/conf --run   # conformance (go test)
lt build -t py examples/validator.lt   # → out/py, package Python
lt build -t go examples/validator.lt   # → out/go, module Go

lt update examples/validator.lt        # re-resolve URL imports, rewrite lt.lock
lt vendor examples/validator.lt        # copy locked remote files into vendor/
```

A static playground for every language rule (50 executable cases + 9 error
cases), with generated TypeScript/Python/Go and, for each target, a runnable
calling example plus the values it prints:

```sh
lt playground          # → out/playground/index.html, open it directly
scripts/playground.sh  # verify 3 targets, record usage stdout + evidence, generate
```

Lihat [docs/playground.md](docs/playground.md).

URL import: `import user "https://raw.githubusercontent.com/..."` — lihat
[docs/package-manager.md](docs/package-manager.md). Tutorial lengkap:
[docs/tutorial.md](docs/tutorial.md); contoh proyek tiga target:
[`examples/library`](examples/library).

Scripts: `scripts/conformance.sh` (TS), `scripts/conformance-py.sh`,
`scripts/conformance-go.sh`.

## Bukan Haxe

Lingua Tungga tidak punya runtime sendiri, tidak punya UI, tidak punya `main`, dan tidak punya IO. Outputnya adalah **library** untuk bahasa host. Tujuannya satu: aturan bisnis dan algoritma yang sama, tanpa ditulis tiga kali dan tanpa drift perilaku.

## Status

| | |
|---|---|
| Fase | M9 — distribusi (CI, rilis binary, installer) |
| Status | **eksperimental** — belum ada janji kompatibilitas sebelum 1.0 |
| Bahasa | ekstensi `.lt` |
| Compiler | Rust, binary `lt` |
| Target | TypeScript ✅, Python ✅, Go ✅ (100% conformance di ketiganya) |
| Lisensi | MIT |

## Dokumen

- [`PLAN.md`](PLAN.md) — keputusan proyek, arsitektur, milestone, risiko
- [`docs/spec.md`](docs/spec.md) — spesifikasi bahasa v0.1
- [`docs/grammar.ebnf`](docs/grammar.ebnf) — grammar formal

## Pengembangan

```sh
./scripts/check.sh          # fmt + clippy + test (yang dijalankan CI tiap push)
./scripts/check.sh --full   # + conformance tiga target + playground
./scripts/release.sh v0.4.0 # build rilis lokal (lihat docs/release.md)
```

Pembagian beban: CI per-push hanya ubuntu + target TypeScript; matriks penuh
(macOS, Windows, Python, Go) hanya saat push ke `main`; playground harian
terpisah di `nightly.yml`. Semua yang berat bisa dijalankan lokal dengan
`--full`.

Build memakai compiler cache [Kache](docs/kache.md) yang datanya disimpan di drive
ADATA. `cargo build`/`cargo test` tidak butuh langkah tambahan; untuk CLI Kache
lihat [docs/kache.md](docs/kache.md).

Struktur saat ini: `lt-syntax` (lexer, AST, parser, formatter), `lt-hir` (loader modul, tabel simbol, typechecker), `lt-ir` (typed IR + lowering), `lt-emit-ts`/`lt-emit-py`/`lt-emit-go` (backend + runtime inline), `lt-pkg` (URL import, lockfile, vendor), `lt-playground`, dan `lt-cli` (binary `lt`). Total 183 test Rust + 45 test conformance di tiga target + 50 kasus playground.
