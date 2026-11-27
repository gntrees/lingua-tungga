# Backend Go

Target ketiga. Conformance suite yang sama (45 test) lulus di sini juga.

> **Status:** dokumen ini menggambarkan emitter **v1** (`class`). Desain v2 (`package` +
> `struct` + fungsi ber-receiver) belum diimplementasikan; baris bertanda →v2 adalah target.
> Lihat [`../v2-decisions.md`](../v2-decisions.md).

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
| `Option<T>` / `Result<T,E>` | `lt.Variant` (`Tag`, `Values []any`, `Display`) |
| `enum` | `lt.Variant` (`Tag`/`Order`/`Values`) + konstruktor `RoleMember(...)` / variabel `RoleAdmin`; enum generik memakai konstruktor `ArgV[T any](...)` dan type argument eksplisit di call site |
| `struct` | `struct` dengan field exported + `NewUser(...)` |
| fungsi ber-receiver →v2 | method pada struct, receiver = parameter pertama |
| fungsi tanpa receiver →v2 | fungsi package-level (`func Format(...)`) |
| `package` →v2 | satu direktori package per package LT (`money/money.go`) |
| `Display`/`Eq` | `lt.Disp`, `lt.Eq` (memakai `LtDisplay`/`LtEq` pada tipe generik & struct) |
| `?` | `if __try.Tag == "err"/"none" { return __try }` |
| `to_string()` | `lt.Disp(value)` |
| tagged template | pemanggilan fungsi package-level dengan `lt.List[string]` + `lt.List[T]`; nilai tidak di-`Disp` |
| `match` | `switch __m.Tag { case "Some": ... }` |
| JSX | pemanggilan `New<Struct>(...)` |

## Struktur paket

```
out/go/
  go.mod                    module linguatungga, go 1.22
  lt/runtime.go
  gen/<package>/<package>.go     satu direktori package per package LT →v2
  gen/<package>/<package>_test.go  (hanya `lt test`)
```

Import antar package memakai path modul: `import m1 "linguatungga/gen/models/user"`.

## Test

```sh
lt test -t go tests/conformance/*.lt --out out/conformance --run
```

`--run` menjalankan `go test ./...`. Script `scripts/conformance-go.sh` juga
menjalankan `go vet ./...` dan memakai toolchain di
`/mnt/A29EE2F79EE2C341/tools/go` bila `go` tidak ada di `PATH`
(`GOCACHE`/`GOPATH` di drive ADATA).

## Batasan v1

- Payload enum/Option/Result disimpan sebagai `any`; emitter menyisipkan type
  assertion yang diketahui dari IR, jadi kesalahan tipe tetap tidak mungkin
  berasal dari program LT yang lolos typecheck.
- Field struct di-export agar idiomatik; immutability ditegakkan oleh compiler
  LT, bukan oleh Go (nilai struct disalin, tapi konsumen Go bisa memutasi
  salinannya).
- Tipe generik kelas dipertahankan (`Box[T]`), tetapi parameter generik dihapus
  dari tipe enum (karena enum memakai `lt.Variant`).

## Formatting

gofmt is context-sensitive (it removes spaces around `+` inside call arguments
but keeps them in `return`), so the emitter does not try to reproduce it byte
for byte. Generated Go is formatted by running `gofmt -w` as part of packaging
(see `scripts/conformance-go.sh` and downstream build scripts); the shipped
package must still satisfy `gofmt -l` being empty.
