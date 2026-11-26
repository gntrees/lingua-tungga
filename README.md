# Lingua Tungga

Satu sumber untuk logika murni dan model data yang berperilaku **identik** di TypeScript, Python, dan Go.

```lt
pub class Price {
  amount: Int
  currency: Str

  pub fn format(self) -> Str {
    `${self.amount / 100}.${self.amount % 100} ${self.currency}`
  }
}
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
lt build -t py examples/validator.lt   # → out/py, package Python
```

## Bukan Haxe

Lingua Tungga tidak punya runtime sendiri, tidak punya UI, tidak punya `main`, dan tidak punya IO. Outputnya adalah **library** untuk bahasa host. Tujuannya satu: aturan bisnis dan algoritma yang sama, tanpa ditulis tiga kali dan tanpa drift perilaku.

## Status

| | |
|---|---|
| Fase | M5 — backend Python |
| Bahasa | ekstensi `.lt` |
| Compiler | Rust, binary `lt` |
| Target | TypeScript ✅, Python ✅, Go (M6) |
| Lisensi | MIT |

## Dokumen

- [`PLAN.md`](PLAN.md) — keputusan proyek, arsitektur, milestone, risiko
- [`docs/spec.md`](docs/spec.md) — spesifikasi bahasa v0.1
- [`docs/grammar.ebnf`](docs/grammar.ebnf) — grammar formal

## Pengembangan

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

Build memakai compiler cache [Kache](docs/kache.md) yang datanya disimpan di drive
ADATA. `cargo build`/`cargo test` tidak butuh langkah tambahan; untuk CLI Kache
lihat [docs/kache.md](docs/kache.md).

Struktur saat ini: `lt-syntax` (lexer, AST, parser, formatter), `lt-hir` (loader modul, tabel simbol, typechecker), `lt-ir` (typed IR + lowering), `lt-emit-ts` (backend TypeScript + runtime inline), dan `lt-cli` (binary `lt`). Total 131 test Rust + 17 test conformance di dua target. Backend Python dan Go menyusul sesuai milestone di `PLAN.md`.
