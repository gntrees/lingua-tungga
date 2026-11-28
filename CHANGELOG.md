# Changelog

## Unreleased

### Added

- **Indexing**: `xs[i]`, `m[k]`, dan `d["k"]` sebagai lookup parsial yang mengembalikan
  `Option<T>`; index di luar batas atau key hilang menjadi `None`, dan `Set` tidak bisa
  di-index. `[` hanya menempel pada ekspresi di baris yang sama.
- **`pkg.Enum.Variant`** di ekspresi dan pattern, serta **`<pkg.Component />`** di JSX. Prefiks
  pada pattern kini diverifikasi terhadap enum yang dicocokkan, bukan diabaikan.
- **`Value` + `json`**: enum prelude `Value` (`Null`, `Bool`, `Int`, `Float`, `Str`,
  `Array(List<Value>)`, `Object(Dict<Value>)`) yang terlihat tanpa import, plus
  `json.parse(Str) -> Result<Value, Str>` dan `json.stringify(Value) -> Str`. Bilangan integral
  tetap `Int`, objek memakai `Dict` sehingga urutan key bertahan, dan float integral dicetak
  dengan `.0` agar `Int`/`Float` tidak hilang.
- Statement kini wajib mulai di baris baru; `xs[0]` tidak lagi diam-diam diparse sebagai dua
  statement.
- Kasus playground `collections/index`, `types/json`, `modules/qualified` dan conformance
  `indexing.lt`, `qualified.lt`, `json.lt`.

### Added

- **Nested patterns**: `Ok(Value.Int(n))`, `Some(Some(x))`, dan pola bertingkat lain kini
  diterima sampai kedalaman bebas. Exhaustiveness hanya diperiksa di level varian teratas; arm
  yang tidak cocok jatuh ke arm berikutnya, dan bila tidak ada yang cocok runtime melaporkan
  `unreachable`.
- **`break` / `continue`** sebagai statement, hanya sah di dalam loop terdekat.
- **`for` atas `Map`/`Dict`** mengiterasi key (`for k in m`), bukan hanya `List`.
- **`map.from(List<Entry<K, V>>)`** membangun map dalam satu kali sortir; `Entry<K, V>` berasal
  dari prelude dan terlihat tanpa import.
- Prelude `value` hanya di-emit bila ada modul yang benar-benar menyebut `Value`/`Entry`
  (`json.*`/`map.from`), sehingga build yang tidak menyentuhnya tidak berubah.
- Kasus playground `control/loops` dan `types/nested_match`, conformance
  `control_flow.lt`, `map_from.lt`, `nested_patterns.lt`.

### Added

- **`Str` indexing**: `s[i]` mengembalikan `Option<Str>` berisi satu code point (O(n)).
- **`for` atas `Set`** mengiterasi elemen dalam urutan `Ord`.
- **Or-pattern** (`E.A(n) | E.B(n) => …`) dan **guard** (`Some(n) if n > 0 => …`). Setiap
  alternatif harus mengikat nama dan tipe yang sama; arm berguard tidak dihitung sebagai
  penutup exhaustiveness.
- Kasus conformance `guards.lt` dan `str_and_set.lt`.

## 0.4.0 — 2026-11-28

### Added

- **`Dict<V>`**: map ber-key `Str` dengan literal `["a": 1, "b": 2]` (kosong `[:]`), urutan
  insertion, duplikat key terakhir-menang tanpa menggeser posisi, dan tujuh method yang
  bentuknya sama dengan `Map` (`len`, `get`, `insert`, `remove`, `keys`, `values`,
  `contains_key`). `Eq`-nya tidak peduli urutan sementara `Display` mengikuti urutan penulisan;
  `cmp`-nya kanonik sehingga `Dict` tetap bisa jadi key `Map`/`Set`.
- Kasus playground `collections/dict` dan conformance `tests/conformance/dicts.lt`.

### Fixed

- Dokumen historis (`PLAN.md`, `docs/v2-decisions.md`) diberi penunjuk bahwa sintaks di
  dalamnya bukan spesifikasi terbaru, dan contoh di spec memakai `map.empty()` (bukan
  `Map.empty()`).

### Changed

- `List`/`Set`/`Map`/`Dict` kini punya perbandingan leksikografis, menggantikan fallback
  perbandingan string display yang membuat `[10]` terurut sebelum `[2]`.
- Tipe hasil inferensi kini diselesaikan setelah body selesai dicek, jadi
  `map.empty().insert(k, v)` mengemit `LMap<K, V>` yang konkret, bukan `unknown`. Bila
  inferensi benar-benar kehabisan petunjuk (`let m = map.empty()`), compiler memberi error
  "cannot infer this type; annotate the binding" alih-alih membiarkan target compiler host
  yang gagal.
- `import ()` kosong sekarang error ("an empty import group does nothing; remove it").
- `Map` lebih cepat: dedup O(n²) menjadi satu lintasan O(n) (sortir adaptif pada array yang
  hampir terurut membuat `insert`/`remove` praktis linear di TypeScript dan Python), dan
  `get`/`contains_key` memakai binary search O(log n).

## 0.3.1 — 2026-11-28

### Added

- Kata kunci kini **soft** di posisi yang sudah pasti nama: setelah `.` semua
  kata kunci boleh (`b.from`, `b.match`), begitu juga nama field, label,
  parameter, dan atribut JSX; nama fungsi/`let`/binding/package serta item import
  menerima `from`, `as`, dan `in`. Jadi `fn from(r: Range)` + `r.from()` sah,
  `from` juga boleh jadi nama package. Kata kunci lain tetap ditolak dengan
  pesan yang menyarankan akhiran `_` (mis. `match_`).

## 0.3.0 — 2026-11-28 (breaking: import syntax, enum output)

### Added

- Playground menampilkan **program pemakaian** di bawah output tiap bahasa: untuk
  setiap target, halaman memuat program yang mengimpor modul hasil emit dan
  memanggil API-nya, plus stdout nyata dari eksekusi program itu. Program
  diturunkan dari IR (test di-replay dengan `expect.*` diganti `print`), jadi
  tidak bisa basi terhadap perubahan API.
- `lt test --usage` mengemit program pemakaian bersama paket + test.
- Kasus playground baru bertema fungsi & variabel: `functions/export_fn`,
  `functions/receiver`, `functions/lambda`, `functions/mutual`,
  `functions/generic`, `bindings/export_let`, `bindings/local_let`.
- Gate baru: kasus executable wajib punya minimal satu deklarasi `export` dan
  minimal satu blok `test`; `scripts/playground.sh` menjalankan setiap program
  pemakaian di ketiga target dan menulis `playground/usage.json`.

### Changed

- Sintaks `import` dipangkas mengikuti Go: `from` dihapus dan path pindah ke
  belakang (`import money { Price, format } "./wallet/money.lt"`), plus blok
  `import ( … )` dengan satu entri per baris dan tanpa koma. Nama package tetap
  ditulis eksplisit dan tetap diverifikasi; destructuring, alias, dan qualifier
  tidak berubah. `lt fmt` kini mengurutkan import berdasarkan path per blok dan
  mempertahankan baris kosong sebagai pemisah sub-blok.
- `import . "path"` (dot import) dan `import _ "path"` (blank import) ditolak
  dengan pesan khusus; `{}` kosong pada import juga error. Kata kunci `from`
  tetap dicadangkan walau tidak lagi dipakai.
- Klarifikasi spec §13: destructuring **tidak** mematikan qualifier — perilaku
  loader selama ini memang begitu, dokumennya yang salah.
- Aturan spec §13 yang sebelumnya hanya tertulis di dokumen kini ditegakkan:
  nama import (qualifier maupun hasil destructuring) yang bentrok dengan
  deklarasi package itu, atau yang diimpor dua kali, menjadi compile error.
- Korpus playground (50 kasus) dan seluruh metadata sidecar kini berbahasa Inggris.
- 24 kasus yang sebelumnya hanya berisi `test` sekarang mengekspor fungsi yang
  dipanggil oleh test-nya, sehingga panel output tidak lagi kosong.
- Output `enum` di ketiga backend kini **satu nama per enum**: varian dipanggil
  sebagai `Shape.Circle(...)` (`Shape.Point` untuk varian tanpa payload), bukan
  lagi simbol terpisah `Shape_Circle`/`shape_circle`/`ShapeCircle`. Fungsi display
  per enum dihapus; display dan urutan varian menjadi metadata yang dipakai
  bersama semua nilai, jadi nilai enum hanya membawa payload.
  - TypeScript: `export type Shape` + `export const Shape` (dua ruang nama, satu
    identifier), `lt.VariantMeta` per varian, 3 properti per nilai.
  - Python: konstruktor menjadi `@staticmethod`/`ClassVar` di dalam kelas enum,
    `_shape_display` dihapus. Varian tanpa payload tetap berupa nilai (`Shape.Point`).
  - Go: `lt.Variant` kehilangan field `Display` dan mendapat `Type`; `LtDisplay`
    merender `Type.Tag(payload)` secara struktural, sehingga `ShapeDisplay` hilang.
    Konstruktor tetap fungsi package-level karena Go tidak mengizinkan nilai
    fungsi generik.

## 0.2.1 — 2026-11-27 (breaking: v2)

Ledger keputusan & alasan: [`docs/v2-decisions.md`](docs/v2-decisions.md). Spesifikasi baru:
[`docs/spec.md`](docs/spec.md).

### Removed

- `class` dihapus. Wadah tunggal sekarang `package`. `class`, `pub`, `static`, dan `self`
  tetap dicadangkan sebagai kata kunci; memakainya menghasilkan error yang menyebut
  penggantinya.
- `test class` dan `test fn` dihapus, diganti `test "judul" { … }` di dalam package.
- `Expect` sebagai class dihapus, diganti pseudo-package `expect`.
- Aturan "tidak ada fungsi bebas" dicabut; fungsi package-level kini satu-satunya bentuk kode.

### Added

- `package nama { … }` sebagai unit nominal dan unit import. Satu file boleh berisi lebih dari
  satu package; satu package = satu modul host = satu direktori package Go.
- `struct` sebagai tipe data; metode menjadi fungsi ber-receiver eksplisit
  (`fn len(p: Point)`, dipanggil `p.len()`).
- `export` sebagai pengganti `pub`, berlaku untuk `struct`, `enum`, fungsi, dan binding.
- Binding level-package (immutable, eager) yang bisa diakses lewat path (`money.default_currency`).
- Path berkualifikasi maksimum 3 segmen: `package.Type`, `package.Type.Variant`, `package.fn`.
- Import package-first dengan destructuring dan alias:
  `import money { Price as P } from "./money.lt"`.

### Changed (breaking)

- Nama package wajib unik per build; dua file dengan nama package sama = compile error.
- Nama dalam satu package wajib unik tanpa memandang huruf besar/kecil.
- Aturan penamaan kini benar-benar ditegakkan compiler (sebelumnya hanya didokumentasikan).
- Shadowing nama import oleh deklarasi lokal = compile error (sebelumnya lokal menang).
- Nama pseudo-class stdlib menjadi pseudo-package huruf kecil: `math`, `map`, `set`, `json`,
  `expect`, `str`, `list`, `option`, `result`.
- `Eq` dan `Ord` tidak bisa ditimpa; hanya `Display` (lewat fungsi ber-receiver `to_string`).
- Interpolasi `${…}` kini menghormati `to_string` milik user. Sebelumnya override hanya berlaku
  untuk pemanggilan `.to_string()` eksplisit, berbeda dari yang dijanjikan `docs/spec.md`.
- JSX: nama tag adalah nama `struct`; tag huruf kecil = compile error.
- Prefix path pada pola `match` tidak lagi diabaikan diam-diam.
- Output ditentukan nama package, bukan nama file `.lt`.

### Not changed (dan alasannya)

- `mut`, assignment, `while`, `for`, `break`, `continue` **dipertahankan**. Menghapusnya menuntut
  perubahan di lima lapisan, guard kedalaman rekursi di setiap fungsi yang diemit, hilangnya
  pola early-exit, dan memperlambat output Python — tanpa melayani tujuan menghapus `class`.
- Nominal typing, tanpa `null`, `Int` = i64 wrapping, `Str` per code point, `Map`/`Set` terurut:
  semua dipertahankan karena inilah yang membuat paritas tiga target bisa dijamin.

### Tagged template & enum (sebelumnya di `Unreleased`)

#### Changed (breaking)

- Tagged template sekarang meneruskan `values` dengan tipe statis, bukan hasil
  `Display`-nya. Signature tag menjadi `(List<Str>, List<T>) -> R`, dan setiap
  `${expr}` harus persis bertipe `T`. Tidak ada lagi konversi implisit
  `Display` → `Str` untuk tagged template.
- Nilai bertipe campuran dinyatakan lewat enum milik user, misalnya
  `enum SqlArg { I(Int), S(Str) }`, lalu tag memakai `List<SqlArg>`.
- `${expr}` pada plain template kini benar-benar di-typecheck, dan `Unit`
  ditolak di posisi interpolasi.
- `to_string() -> Str` tersedia untuk semua nilai bertipe `Display`. Gunakan
  ini (atau enum payung) sebagai jalur migrasi tag lama bertipe `List<Str>`.
  Method `to_string` milik user menang atas versi bawaan.

#### Fixed

- Interpolasi `${expr}` pada tagged template sebelumnya tidak pernah
  di-typecheck; variabel tak dikenal atau tipe salah bisa lolos `lt check`.
- Enum generik menghasilkan Go yang tidak valid ketika payload memakai type
  parameter. Konstruktor sekarang generik dan call site mengirim type argument
  eksplisit.
- Varian nullary pada enum generik menghasilkan `const X<T>` yang ilegal di
  TypeScript; type parameter dihapus dan nilainya diberi tipe `any`.
