# Changelog

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
