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
lt check src/       # resolve import + typecheck seluruh graf
lt fmt src/         # format kanonik (idempoten)
lt fmt --check src/ # exit 1 kalau ada file belum terformat
lt ast src/a.lt     # dump AST untuk debugging
```

## Bukan Haxe

Lingua Tungga tidak punya runtime sendiri, tidak punya UI, tidak punya `main`, dan tidak punya IO. Outputnya adalah **library** untuk bahasa host. Tujuannya satu: aturan bisnis dan algoritma yang sama, tanpa ditulis tiga kali dan tanpa drift perilaku.

## Status

| | |
|---|---|
| Fase | M2 — resolusi modul + typechecker (`lt-hir`) |
| Bahasa | ekstensi `.lt` |
| Compiler | Rust, binary `lt` |
| Target | TypeScript, Python, Go (100% conformance) |
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

Struktur saat ini: `crates/lt-syntax` (lexer, AST, parser, formatter, diagnostik) dan `crates/lt-hir` (loader modul, tabel simbol, typechecker). `crates/lt-cli` menyediakan binary `lt`. Total 109 test. IR dan emitter menyusul sesuai milestone di `PLAN.md`.
