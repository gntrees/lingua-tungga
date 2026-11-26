# Backend Go

Target ketiga. Conformance suite yang sama (17 test) lulus di sini juga.

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
| `enum` | `lt.Variant` (`Tag`/`Order`/`Values`) + konstruktor `RoleMember(...)` / variabel `RoleAdmin` |
| `class` | `struct` dengan field exported + `NewUser(...)` |
| method instance | method pada struct, receiver `self` |
| method tanpa `self` | fungsi package `UserAnonymous()` |
| `Display`/`Eq` | `lt.Disp`, `lt.Eq` (memakai `LtDisplay`/`LtEq` pada tipe generik & struct) |
| `?` | `if __try.Tag == "err"/"none" { return __try }` |
| `match` | `switch __m.Tag { case "Some": ... }` |
| JSX | pemanggilan `New<Class>(...)` |

## Struktur paket

```
out/go/
  go.mod                    module linguatungga, go 1.22
  lt/runtime.go
  gen/<module>/<base>.go    package <base>
  gen/<module>/<base>_test.go  (hanya `lt test`)
```

Import antar modul memakai path modul: `import m1 "linguatungga/gen/models/user"`.

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
