# Lingua Tungga — Rencana Proyek

> Dokumen ini adalah hasil sesi desain yang dikunci per keputusan. Semua keputusan di bawah sudah disetujui; yang belum dikunci dicantumkan eksplisit di bagian **Keputusan Mikro yang Belum Dikunci**, supaya tidak ada asumsi yang tersembunyi.

- Nama bahasa: **Lingua Tungga**
- Ekstensi file: `.lt`
- Compiler: Rust, binary `lt`
- Runtime: `lingua-tungga-runtime` (di-inline ke output, bukan dependency eksternal)
- Lisensi: **MIT**
- Target v1: **TypeScript, Python, Go** (100% conformance, bukan "hampir")

---

## 1. Positioning

Lingua Tungga adalah **portable core language**: satu sumber untuk logika murni dan model data yang harus berperilaku identik di beberapa bahasa host.

**Lingua Tungga bukan:**

| Bukan | Alasan |
|---|---|
| Haxe | Tidak ada UI, tidak ada aplikasi lintas platform, tidak ada GC/runtime sendiri |
| Code generator/SDK generator | Outputnya library nyata yang dipakai host language, bukan stub |
| Bahasa scripting | Tidak ada IO, tidak ada `main`, output = library |
| Bahasa dinamis | Nominal typing, tanpa `any`, tanpa `Dynamic` |
| DSL per-target | Tidak ada kode kondisional per target di library portable |

**Kasus pakai kanonik:** aturan bisnis, algoritma, validasi, model domain yang harus ada identik di backend Go, service Python, dan frontend TypeScript — tanpa triple maintenance dan tanpa drift perilaku.

---

## 2. Keputusan yang Sudah Dikunci

### 2.1 Semantik inti

| # | Keputusan |
|---|---|
| 1 | Scope v1: **pure computation + data + templating**. Nol IO, nol async, nol concurrency, nol global mutable state. |
| 2 | Output: **kode user readable**, runtime inline tidak perlu readable. |
| 3 | **Strict semantics**: fitur yang tidak bisa dipetakan ke suatu target = compile error, bukan perbedaan diam-diam. |
| 4 | Nominal typing. **Tanpa null bebas** (`Option<T>`), **tanpa exception sebagai alur normal** (`Result<T, E>`). |
| 5 | **Semua immutable.** `mut` hanya untuk variabel lokal. Field class tidak pernah mutable. |
| 6 | Generik: hanya pada `class`, `enum`, method, dengan constraint built-in (`Eq`, `Ord`, `Display`). Tanpa higher-kinded types. |
| 7 | **Tanpa interface user-defined** di v1. Hanya built-in trait. |
| 8 | **Tanpa inheritance**, tanpa `override`, tanpa `abstract`. |
| 9 | **Tanpa fungsi bebas sama sekali.** Semua kode hidup di dalam `class` atau `enum`. |

### 2.2 Aritmetika & string

| # | Keputusan |
|---|---|
| 10 | `Int` = i64, **overflow wrapping** (didefinisikan, bukan UB). |
| 11 | `Float` = f64 IEEE-754, `NaN` ikut standar. |
| 12 | Tanpa konversi implisit `Int` ↔ `Float`. |
| 13 | `Str` = immutable, diindeks per **Unicode code point**, bukan byte. |
| 14 | String literal hanya `"..."` (tanpa `'`, tanpa char type). |

### 2.3 Struktur program

| # | Keputusan |
|---|---|
| 15 | Unit yang bisa di-import **hanya `pub class`** (dan `pub enum` untuk tipe data). `fn` top-level tidak ada. |
| 16 | Default private; `pub` eksplisit untuk class, enum, dan method. |
| 17 | Tidak ada namespace/blok module. Path file bukan namespace — import menunjuk langsung ke file. |
| 18 | **Import cycle dilarang** (Go tidak mendukung, dan melingkar = desain salah). |
| 19 | Konstruksi **named arguments wajib**: `User(name: "a", age: 1)`. Posisional ditolak. |
| 20 | `static fn` boleh, wajib murni (tidak menyimpan state). |

### 2.4 Fitur sintaks

| # | Keputusan |
|---|---|
| 21 | Template literal backtick dengan `${expr}`, `expr` wajib `Display`, hasil selalu `Str`. Multiline verbatim, tanpa auto-dedent. |
| 22 | **Tagged template**: ``tag`a ${x} b` `` → `tag(parts: List<Str>, values: List<Str>)`, invariant `parts.len() == values.len() + 1`, nilai di-`Display`-kan sebelum masuk tag. Tidak ada built-in tag. |
| 23 | **JSX/XML = literal typed tree**, nama tag = tipe/class, atribut = named argument, children = children. Bukan UI framework. |
| 24 | Tidak ada tipe `Dynamic` bawaan. Kebutuhan heterogen diselesaikan dengan `enum Value` biasa di library. |

### 2.5 Packaging & import

| # | Keputusan |
|---|---|
| 25 | Import URL langsung, tanpa config: `import User from "https://raw.githubusercontent.com/..."`. |
| 26 | Resolusi pertama menulis **commit SHA + checksum** ke `lt.lock`; build berikutnya nol network. |
| 27 | `lt.lock` **wajib di-commit**. `lt update` untuk refresh. |
| 28 | Dep di dalam file remote di-resolve relatif ke URL asalnya, dikunci rekursif. |
| 29 | Tanpa build script, tanpa hook, tanpa kode per-target di package published. |
| 30 | `lt vendor` untuk build offline/CI. |

### 2.6 Output & runtime

| # | Keputusan |
|---|---|
| 31 | **Library-first**: tidak ada `main`. Output per target = paket siap pakai di `out/<target>/` dengan manifest (`package.json`, `pyproject.toml`, `go.mod`). |
| 32 | Runtime **inline**: satu file `_lingua_rt.<ext>` (Go: `rt.go` dalam package yang sama) per paket output, di-generate sekali. |
| 33 | Kode user wajib lulus checker native: `tsc --strict`, `ruff` + `mypy --strict`, `go vet` + `gofmt -l` kosong. |
| 34 | `Int` di TypeScript = `bigint` + `BigInt.asIntN(64, …)`. Strict menang atas keindahan. |
| 35 | `enum` di Go = tagged struct + type switch, bukan interface + refleksi. |
| 36 | Generik di Go = type parameter native (Go 1.18+), tanpa erasure/simulasi. |
| 37 | `Display` di-derive otomatis (bisa ditimpa manual). `Eq`/`Ord` juga di-derive. |

### 2.7 Proses

| # | Keputusan |
|---|---|
| 38 | Dua lapis test: conformance `.lt` (dijalankan di 3 target, runner native) + golden/IR snapshot (sisi Rust). |
| 39 | **Tidak boleh merge emitter baru** kalau conformance target itu belum 100% hijau. |
| 40 | Monorepo: compiler Rust + 3 runtime + conformance suite. |
| 41 | Publik sejak M1. |

---

## 3. Contoh Sintaks (target akhir v1)

```lt
// src/models/user.lt
import Address from "./address.lt"

pub enum Role {
  Admin
  Member(Int)          // level
  Guest
}

pub class User {
  name: Str
  age: Int
  role: Role
  address: Option<Address>

  pub fn greet(self) -> Str {
    `Hi ${self.name}, you are ${self.age}`
  }

  pub fn label(self) -> Str {
    match self.role {
      Role.Admin    => "admin"
      Role.Member(n) => `member-${n}`
      Role.Guest    => "guest"
    }
  }

  pub fn city(self) -> Str {
    match self.address {
      Some(a) => a.city
      None    => "-"
    }
  }

  static fn anonymous() -> User {
    User(name: "?", age: 0, role: Role.Guest, address: None)
  }
}
```

```lt
// src/validation.lt
import User from "./models/user.lt"

pub class Validator {
  pub fn check(u: User) -> Result<User, Str> {
    if u.age < 0 {
      return Err("age must be >= 0")
    }
    if u.name.len() == 0 {
      return Err("name required")
    }
    Ok(u)
  }

  pub fn check_all(users: List<User>) -> List<Result<User, Str>> {
    users.map(fn(u) => Validator.check(u))
  }
}
```

```lt
// src/sql.lt  (tagged template)
pub class Sql {
  pub fn query(parts: List<Str>, values: List<Str>) -> Query {
    // bagian pembuatan query aman, sesuai strategi library
  }

  pub fn find(id: Int) -> Query {
    Sql.query`SELECT * FROM users WHERE id = ${id}`
  }
}
```

```lt
// src/view.lt  (JSX = typed tree)
import User from "./models/user.lt"
import Element from "./element.lt"

pub class UserView {
  pub fn render(u: User) -> Element {
    <Element tag="div" class_name="user">
      <Element tag="span">{u.name}</Element>
      <Element tag="span">{u.age}</Element>
    </Element>
  }
}
```

Catatan: bentuk persis `match`, lambda, dan JSX di atas masih **belum dikunci** (lihat §11). Yang sudah dikunci adalah semantiknya.

Cara pakai:

```
$ lt build -t go
→ out/go/go.mod, out/go/rt.go, out/go/user/user.go, ...
$ cd ../my-service && go get ./...   # package siap di-import
```

---

## 4. Arsitektur Compiler

```
.lt source
   │
   ├─ lt-syntax ──────────→ token + AST + span
   │
   ├─ lt-hir ─────────────→ resolve import, desugar
   │                        (template, tagged template, JSX, `?`)
   │
   ├─ lt-typeck ──────────→ semua simpul bertipe final, tanpa inferensi tersisa
   │
   ├─ lt-ir ──────────────→ IR target-neutral (typed tree)
   │                        └─ dump S-expr stabil (debug + golden test)
   │
   └─ lt-emit-{ts,py,go} ─→ teks target, emitter murni (IR → String)
```

**IR wajib.** Tanpa IR, tiga emitter berarti tiga salinan logika semantik; untuk target ke-4 dst. ini mustahil. Dengan IR, logika "apa arti `Result`" ditulis sekali dan setiap emitter hanya menerjemahkan bentuk.

Aturan emitter:

- Fungsi murni `IR -> String`. Tidak menyentuh filesystem/network, tidak menyimpan state global.
- Urutan emit **deterministik** (map diurutkan berdasarkan key, bukan hash order) supaya golden file stabil.
- Penamaan output ditentukan di emitter (bukan di parser): `calculate_total` → `calculate_total` (Py/Go), `calculateTotal` (TS).

---

## 5. Tabel Pemetaan LT → Target

| Konstruksi LT | TypeScript | Python | Go |
|---|---|---|---|
| `Int` (i64 wrap) | `bigint` + `BigInt.asIntN` | `int` + masking `_i64()` | `int64` |
| `Float` | `number` | `float` | `float64` |
| `Str` (code point) | `string` (via `Array.from` untuk index/len) | `str` | `string` + `[]rune` helper |
| `Bool` | `boolean` | `bool` | `bool` |
| `Option<T>` | `Opt<T>` union di `_rt` | `Opt[T]` dataclass | `Opt[T]` struct generik |
| `Result<T,E>` | `Res<T,E>` union | `Res[T,E]` dataclass | `Res[T,E]` struct generik |
| `?` (propagasi) | early return `if (isErr(x)) return x` | early return typed | `if r.IsErr() { return r }` |
| `class` | frozen object + factory + fungsi method | `@dataclass(frozen=True)` + method | `struct` + method value receiver |
| `enum` | discriminated union (`{tag, ...}`) | `@dataclass(frozen=True)` per varian + union type | tagged struct + type switch |
| `List<T>` | `LList<T>` (ReadonlyArray wrapper) | immutable tuple-based class | `LList[T]` slice wrapper (copy-on-write) |
| `Map<K,V>` | `LMap<K,V>` wrapper | immutable dict wrapper | `LMap[K,V]` map wrapper |
| `Set<T>` | `LSet<T>` | `frozenset` wrapper | `LSet[T]` |
| generik | native | `TypeVar` + `Generic` | type parameter native |
| `Display` | `toString()` | `__str__` | `String()` |
| `Eq` | `equals()` (deep) | `__eq__` | `Equal()` |
| `Ord` | `cmp()` | `__lt__` + `__eq__` | `Compare()` |
| template `` `a ${x}` `` | template literal / concat | f-string | `fmt.Sprintf` / concat |
| tagged template | `tag([...parts], [...vals])` | `tag([...], [...])` | `tag(parts, vals)` |
| JSX | konstruktor class | konstruktor dataclass | struct literal |
| import URL | tidak berlaku di output (sudah di-flatten) | idem | idem |
| test class | file `*.test.ts` (vitest) | `test_*.py` (pytest) | `*_test.go` (`go test`) |

**Catatan risiko pada tabel ini:**

- `bigint` di TS membuat JSON encode manual dan output kurang familiar — ini konsekuensi langsung dari keputusan i64-wrap.
- Python butuh masking i64 di setiap operasi aritmetika (`_i64(x)`), yang memperlambat eksekusi. Dapat diterima untuk library logika.
- `List`/`Map` immutable di Go butuh copy-on-write; alokasi lebih banyak. Terima dulu; optimasi nanti tanpa mengubah semantik.

---

## 6. Runtime Inline

Satu file per paket output:

| Target | File | Isi |
|---|---|---|
| TS | `out/ts/src/_lingua_rt.ts` | `Opt`, `Res`, `LList`, `LMap`, `LSet`, `i64`, helper Display/Eq/Ord |
| Python | `out/py/lingua_tungga/_rt.py` | `Opt`, `Res`, `LList`, `LMap`, `LSet`, `_i64`, helper |
| Go | `out/go/rt.go` | `Opt[T]`, `Res[T,E]`, `LList[T]`, `LMap[K,V]`, `LSet[T]` |

Sumber runtime ada di `runtimes/{ts,py,go}/` di repo dan di-embed ke binary `lt` lewat `include_str!`. Jadi `lt` adalah satu binary tanpa file pendamping.

**Kontrak runtime:** nama dan bentuk API di ketiga runtime identik secara semantik. Setiap fungsi runtime punya test conformance sendiri, karena bug di runtime = bug di semua program pengguna.

---

## 7. Package & Import

### 7.1 Deklarasi

```lt
import User from "./models/user.lt"
import Post from "https://raw.githubusercontent.com/foo/bar/v1.2.0/src/post.lt"
import Validator from "https://raw.githubusercontent.com/foo/bar/v1.2.0/src/validator.lt"
```

Tidak ada `lt.toml` yang wajib untuk konsumen. Package **publisher** boleh punya `lt.toml` (nama, versi, lisensi) — itu metadata, bukan syarat resolusi.

### 7.2 Lockfile

```toml
# lt.lock — WAJIB di-commit
[[package]]
url    = "https://raw.githubusercontent.com/foo/bar/v1.2.0/src/post.lt"
commit = "8f3c...e2"
sha256 = "a91b..."
```

- Build pertama: fetch → resolve tag/branch → commit SHA → checksum → tulis lock.
- Build berikutnya: verifikasi checksum, **nol network**.
- URL menunjuk branch → warning (isinya bisa berubah), tapi lock melindungi.
- `lt update` = refresh semua atau satu URL.

### 7.3 Cache & vendor

- Cache: `~/.cache/lt/<host>/<owner>/<repo>/<sha>/...`
- `lt vendor` → `vendor/` di proyek, dipakai otomatis kalau ada.
- Batas keamanan: lock + checksum mencegah **perubahan** diam-diam, bukan mencegah kode jahat. Ini didokumentasikan di README.

### 7.4 Batasan

- Import cycle = compile error (dengan pesan yang menunjukkan siklusnya).
- Nama class yang di-import harus ada di file target; nama beda = error, tidak ada alias.
- Tidak ada re-export.

---

## 8. Strategi Test & Conformance

### Lapis 1 — Conformance `.lt`

```lt
// tests/conformance/user_test.lt
pub test class UserTest {
  fn greet_format() {
    let u = User(name: "Ana", age: 30, role: Role.Guest, address: None)
    Expect.equal(u.greet(), "Hi Ana, you are 30")
  }

  fn option_none_city() {
    let u = User.anonymous()
    Expect.equal(u.city(), "-")
  }
}
```

`lt test -t ts` → generate `out/ts/tests/*.test.ts` → jalankan `vitest`.
`lt test -t py` → generate `test_*.py` → jalankan `pytest`.
`lt test -t go` → generate `*_test.go` → jalankan `go test`.

**Satu file `.lt` = tiga eksekusi.** Inilah bukti "100%".

### Lapis 2 — Golden & IR snapshot (Rust)

- `cargo test` membandingkan output emitter byte-identik dengan snapshot di `tests/golden/<target>/`.
- Snapshot IR di `tests/golden/ir/`.
- Regresi terdeteksi tanpa menjalankan 3 runtime.

### Definisi "target selesai"

1. Seluruh conformance `.lt` lulus di target itu.
2. Output lulus `tsc --strict` / `ruff` + `mypy --strict` / `go vet` + `gofmt -l` kosong.
3. Golden file byte-identik.
4. Tidak ada fitur yang di-emit diam-diam sebagai "belum didukung" — harus compile error.

---

## 9. Milestone

Setiap milestone harus hijau sebelum lanjut. Estimasi asumsi ~10 jam/minggu solo.

### M1 — Frontend (`lt-syntax`)
- Tokenizer: string, template, tagged template, JSX, komentar, angka.
- Parser rekursif-descent + AST + span akurat.
- Diagnostik: `file:line:col`, cuplikan baris, panah kolom, pesan "expected X, found Y".
- `lt fmt` idempoten; `lt parse --dump-ast`.
- **Selesai:** 200+ file `.lt` (valid & invalid) menghasilkan error yang benar; formatter idempoten; snapshot test lengkap.
- Estimasi: 4–8 minggu.

### M2 — HIR + Typechecker (`lt check`)
- Resolve import (lokal dulu), deteksi import cycle.
- Desugar: template, tagged template, JSX, `?`.
- Typechecker nominal, `Option`/`Result`, generik + constraint, derive `Eq`/`Ord`/`Display`.
- **Selesai:** semua conformance `.lt` lolos typecheck; error berkualitas sama dengan M1.
- Estimasi: 8–12 minggu.

### M3 — IR + emitter TypeScript
- IR typed-tree + dump S-expr.
- `lt-emit-ts` + runtime TS inline.
- `lt build -t ts`.
- **Selesai:** output TS lulus `tsc --strict`; golden TS ada.
- Estimasi: 6–10 minggu.

### M4 — Conformance TypeScript 100%
- Suite conformance lengkap (bukan subset).
- **Selesai:** aturan §8 terpenuhi untuk TS. Rilis binary `lt` pertama.
- Estimasi: 3–6 minggu.

### M5 — Emitter + runtime Python 100%
- Estimasi: 6–10 minggu.

### M6 — Emitter + runtime Go 100%
- Estimasi: 6–10 minggu.

### M7 — Package manager
- Fetch URL/git, `lt.lock`, checksum, cache, `lt vendor`, `lt update`.
- **Selesai:** build reproducible offline; import cycle & checksum mismatch ditolak dengan pesan jelas.
- Estimasi: 4–8 minggu.

### M8 — Polish
- Dokumentasi bahasa + tutorial + contoh nyata (satu library domain dipublikasikan ke npm/PyPI/Go module).
- Playground statis (WASM) — opsional.
- **Selesai:** orang lain bisa menyelesaikan tutorial tanpa bertanya.
- Estimasi: 4–8 minggu.

**Total realistis sampai M6:** ~37–64 minggu part-time. Ini alasan target dipangkas ke 3.

---

## 10. Struktur Repo

```
lingua-tungga/
├── Cargo.toml                  # workspace
├── crates/
│   ├── lt-syntax/              # lexer, parser, AST, diagnostik
│   ├── lt-hir/                 # resolve import, desugar, lowering
│   ├── lt-typeck/              # type checker
│   ├── lt-ir/                  # IR + dump S-expr
│   ├── lt-emit-ts/
│   ├── lt-emit-py/
│   ├── lt-emit-go/
│   ├── lt-pkg/                 # lockfile, fetch, checksum, cache, vendor
│   └── lt-cli/                 # binary `lt`
├── runtimes/
│   ├── ts/                     # di-embed via include_str!
│   ├── py/
│   └── go/
├── tests/
│   ├── conformance/            # *.lt
│   └── golden/{ir,ts,py,go}/   # snapshot
├── examples/
└── docs/
```

Dependensi Rust yang direkomendasikan: `rowan` atau AST manual + `logos` (lexer), `ariadne`/`miette` (diagnostik), `insta` (snapshot), `clap` (CLI), `reqwest` + `sha2` (fetch/checksum), `gix` atau shell-out `git`.

---

## 11. Keputusan Mikro — Sudah Dikunci

Diputuskan sebelum M1; detail lengkap di [`docs/spec.md`](docs/spec.md) dan [`docs/grammar.ebnf`](docs/grammar.ebnf).

| # | Keputusan |
|---|---|
| 1 | `match` ada, wajib exhaustive. Pola: `_`, `None`, `Variant`, `Variant(p, …)`. Tanpa guard, tanpa destructuring class. |
| 2 | Lambda `fn(x) => expr`; bentuk blok `fn(x) => { … }`. Bukan `\|x\| …`. |
| 3 | JSX: atribut = named arg; children hanya sah jika class punya field `children`; self-closing = tanpa children; atribut bernama kata kunci dilarang. |
| 4 | `?` hanya menyilang tipe yang sama: `Option` di method `Option`, `Result` di method `Result`. Tidak boleh campur. |
| 5 | Escape: `\n \t \r \\ \" \` \$ \u{…}`. Lainnya = compile error. |
| 6 | Formatter: 2 spasi, 100 kolom, trailing comma multiline, satu baris kosong antar member, tanpa `;`, idempoten. |
| 7 | Test: `test class X { test fn name() { … } }`; `fn` biasa = helper. Tanpa atribut. |
| 8 | Output 1:1 dengan source, struktur direktori dipertahankan, nama file `snake_case` (ditegakkan compiler). |
| 9 | `pub enum` bisa di-import, setara `pub class`. |
| 10 | Tipe dari file yang tidak di-import = compile error. Tanpa re-export, tanpa alias. |

---

## 12. Risiko

| Risiko | Dampak | Mitigasi |
|---|---|---|
| Scope creep ke "bahasa sungguhan" | Proyek tidak pernah selesai | Daftar permanen di §2.1 + aturan no-merge |
| `bigint` di TS membuat output aneh | Adopsi turun | Dokumentasikan; sediakan helper JSON di runtime |
| i64 masking di Python lambat | Performa buruk | Batasi pemakaian ke operasi aritmetika; benchmark awal |
| Go import cycle vs domain model natural | Desain class dipaksa | Deteksi dini di typechecker; pesan yang menyarankan penggabungan file |
| Runtime inline = 3 salinan kode yang harus sinkron | Bug senyap | Conformance test untuk runtime, golden per target |
| Solo + 3 target | Burnout | Milestone kecil, rilis setelah M4 |
| URL import = supply chain | Eksekusi kode asing | Lock + checksum + dokumentasi batas keamanan |
| Stagnasi setelah M6 | Proyek mati | Tetapkan definisi "v1 selesai" = M6, sisanya bonus |

---

## 13. Yang Perlu Disiapkan (Checklist Pra-Kode)

**Pengetahuan**
- Rust: cargo workspace, parser/visitor, error handling dengan span (`ariadne`), `insta`.
- Compiler dasar: lexer, recursive-descent/Pratt, symbol resolution, type checking nominal.
- Go 1.18+ generics, `gofmt`/`go vet`, test table-driven.
- TypeScript `--strict`, `bigint`, vitest.
- Python typing (`Protocol`, `TypeVar`, `dataclass(frozen=True)`, `mypy --strict`), pytest.
- Git plumbing untuk resolusi tag → commit SHA.

**Tooling lokal**
- Rust stable + clippy, Node 20 + pnpm, Python 3.11+ + uv, Go 1.22+, git, gh.

**Aset repo**
- Repo publik sejak M1, MIT `LICENSE`, `README.md` (positioning §1), `docs/spec.md` kosong yang diisi bertahap per milestone.
- `tests/conformance/` dibuat sejak M1 walau baru dijalankan di M4.

**Urutan kerja pertama (M1, minggu 1–2)**
1. `cargo new` workspace + crate `lt-syntax`.
2. Tulis grammar dalam `docs/grammar.ebnf` sebelum menulis parser.
3. Tokenizer + test-nya.
4. Baru parser.

---

## 14. Langkah Berikutnya yang Konkret

1. Kunci 10 keputusan mikro di §11 (satu sesi).
2. Tulis `docs/spec.md` dari §2 + §11.
3. Mulai M1: grammar EBNF → tokenizer → parser → diagnostik → formatter.
