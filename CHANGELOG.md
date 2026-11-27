# Changelog

## v2 — BREAKING (2026-11-27)

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

---

## Unreleased

### Changed (breaking)

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

### Fixed

- Interpolasi `${expr}` pada tagged template sebelumnya tidak pernah
  di-typecheck; variabel tak dikenal atau tipe salah bisa lolos `lt check`.
- Enum generik menghasilkan Go yang tidak valid ketika payload memakai type
  parameter. Konstruktor sekarang generik dan call site mengirim type argument
  eksplisit.
- Varian nullary pada enum generik menghasilkan `const X<T>` yang ilegal di
  TypeScript; type parameter dihapus dan nilainya diberi tipe `any`.
