# Backend Go

Target ketiga. Status: **v2 — terverifikasi**; `go vet` bersih dan 13/13 paket conformance
lulus di `go test`.

## Representasi

| LT | Go |
|---|---|
| `Int` (i64) | `int64` — overflow wrapping sudah bawaan Go |
| `Float` | `float64`; pembagian lewat `lt.FDiv` agar `1.0/0.0` tidak ditolak compiler sebagai konstanta |
| `Str` | `string`; operasi code point memakai `lt.StrLen/StrSlice/StrChars/...` |
| `Bool` | `bool` |
| `List<T>` | `lt.List[T]` |
| `Map<K,V>` | `lt.Map[K,V]` (entry terurut) |
| `Set<T>` | `lt.Set[T]` |
| `Option<T>` / `Result<T,E>` | `lt.Variant` (`Tag`, `Order`, `Type`, `Values`) |
| `enum` | `lt.Variant` dengan `Type` = nama enum + konstruktor `EnumVariant(...)`; display struktural dari `Type`/`Tag`/`Values`, tanpa fungsi display per enum |
| `struct` | `struct` dengan field exported + `NewUser(...)` |
| fungsi package-level | `func Name(...)` untuk `export`, `func name(...)` (camelCase) untuk private |
| binding package | `var Name T = …` / `var name T = …` |
| `package` | satu direktori package: `gen/<package>/<package>.go` |
| fungsi ber-receiver | tetap fungsi package-level; `p.len()` menjadi `len(p)` saat lowering |
| `Display`/`Eq`/`Ord` | `lt.Disp`, `lt.Eq`, `lt.Cmp` lewat method `LtDisplay`/`LtEq`/`LtCmp` pada struct; `lt.Variant` merender display-nya sendiri dari `Type`/`Tag`/`Values` |
| `?` | `if __try.Tag == "err"/"none" { return __try }` |
| `to_string()` | `lt.Disp(value)` |
| tagged template | pemanggilan fungsi package-level dengan `lt.List[string]` + `lt.List[T]` |
| `match` | `switch __m.Tag { case "Some": ... }` |
| JSX | pemanggilan `New<Struct>(...)` |
| `expect.eq` dkk | `lt.ExpectEqual` / `ExpectNotEqual` / `ExpectTrue` / `ExpectFalse` |
| `test "judul"` | `func TestJudul(t *testing.T)` di `<package>_test.go`, **dalam package yang sama** |

Test di-emit sebagai `package <name>` (bukan `<name>_test`) supaya helper private di package itu
tetap bisa dipanggil — Go tidak mengekspor identifier huruf kecil ke package lain.

## Struktur paket

```
out/go/
  go.mod                    module linguatungga, go 1.22
  lt/runtime.go
  gen/<package>/<package>.go        satu direktori per package LT
  gen/<package>/<package>_test.go   hanya dari `lt test`
```

Import antar package memakai path modul: `import m1 "linguatungga/gen/user"`.

## Test

```sh
lt test -t go tests/conformance/*.lt --out out/conformance --run
```

`--run` menjalankan `go test ./...`. Script `scripts/conformance-go.sh` juga menjalankan
`go vet ./...` dan memakai toolchain lokal di `/mnt/A29EE2F79EE2C341/tools/go` bila `go` tidak ada
di `PATH`.

## Batasan

- Payload enum/`Option`/`Result` disimpan sebagai `any`; emitter menyisipkan type assertion yang
  diketahui dari IR, jadi kesalahan tipe tetap tidak mungkin berasal dari program LT yang lolos
  typecheck.
- Field struct di-export agar idiomatik; immutability ditegakkan compiler LT, bukan Go.
- Parameter generik dihapus dari tipe enum (karena enum memakai `lt.Variant`).
- `json`/`Value` belum ada; `Ordering` belum tersedia di permukaan.
- Builtin hanya tersedia sebagai receiver (`xs.Map(…)`), bukan `list.map(xs, …)`.

## Formatting

gofmt peka konteks (menghapus spasi di sekitar `+` dalam argumen pemanggilan tetapi
mempertahankannya di `return`), jadi emitter tidak meniru byte-per-byte. Output Go diformat dengan
menjalankan `gofmt -w` saat packaging; paket yang dikirim tetap wajib memenuhi `gofmt -l` kosong.
