# Changelog

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
