# Backend TypeScript

Target pertama, dipakai untuk memvalidasi IR dan semantik. Status: **v2 — terverifikasi**;
`tsc --strict` bersih dan 45 test conformance lulus di `vitest`.

## Representasi

| LT | TypeScript |
|---|---|
| `Int` (i64) | `bigint` + `BigInt.asIntN(64, …)` lewat `lt.iadd/isub/imul/idiv/irem/ineg` |
| `Float` | `number` |
| `Str` | `string`; index/len dihitung per code point (`[...s]`) |
| `Bool` | `boolean` |
| `List<T>` | `lt.LList<T>` (immutable, copy-on-write) |
| `Map<K,V>` | `lt.LMap<K,V>` (entry terurut `Ord`, lookup binary search O(log n)) |
| `Dict<V>` | `lt.LDict<V>` (key `string`, urutan insertion, indeks `Map`) |
| `Set<T>` | `lt.LSet<T>` |
| `Option<T>` | `lt.Opt<T>` = union `{tag:"some"\|"none", values, __lt_meta}` |
| `Result<T,E>` | `lt.Res<T,E>` = union `{tag:"ok"\|"err", …}` |
| `enum` | satu identifier: `export type Enum` + `export const Enum`, varian dipanggil `Enum.Variant(...)`; urutan & display varian ada di `Enum_variant` yang dipakai bersama |
| `struct` | `class` dengan field `readonly`, konstruktor objek `{ field: … }`, `__lt_eq`, `__lt_cmp`, `__lt_to_string` |
| fungsi package-level | `export function name(...)` |
| fungsi private | `export function _name(...)` — tetap diekspor agar file test bisa memanggilnya, tetapi ditandai `_` sebagai internal |
| binding package | `export const name: T = …` (private: `_name`) |
| `package` | satu file modul: `src/<package>.ts` |
| fungsi ber-receiver | tetap fungsi modul; `p.len()` diturunkan menjadi `len(p)` saat lowering |
| `Display` | `lt.disp(value)`; `to_string` milik package pemilik tipe dipanggil oleh `__lt_to_string` |
| `Eq` / `Ord` | `lt.eq(a, b)` / `lt.cmp(a, b)` |
| `?` pada `Result`/`Option` | `const t = …; if (t.tag === "err"/"none") return t;` |
| template | konkatenasi `+ lt.disp(...)` |
| tagged template | pemanggilan fungsi package-level dengan `LList<string>` + `LList<T>`; nilai tidak di-`disp` |
| `match` | `switch (value.tag)` |
| JSX | `new Struct({ field: …, children: new LList([...]) })` |
| `expect.eq` dkk | `lt.expectEqual` / `expectNotEqual` / `expectTrue` / `expectFalse` |
| `test "judul"` | `it("judul", …)` di `tests/<package>.test.ts` |

## Struktur paket

```
out/ts/
  _lingua_rt.ts        runtime inline (di-embed dari runtimes/ts/runtime.ts)
  package.json
  tsconfig.json        strict, ES2022, noEmit
  src/<package>.ts     satu file per package LT
  tests/<package>.test.ts  hanya dari `lt test`
```

Import antar package memakai namespace `m<id>` (`import * as m1 from "./user"`), sehingga tidak
ada tabrakan nama. Alias runtime selalu `lt`, dan deklarasi pengguna bernama `lt` di-mangle
menjadi `lt_`.

## Penamaan

- field, fungsi, parameter, variabel lokal: `snake_case` → `camelCase`
- fungsi/binding private: prefiks `_` (`_doubleTwice`)
- struct, enum, varian: tetap `PascalCase`
- kata kunci TypeScript diberi akhiran `_`
- judul test dipakai apa adanya di `it("…")`

## Test

```sh
lt test -t ts tests/conformance/*.lt --out out/conformance --run
```

`--run` menjalankan `vitest`. `tsconfig.json` menyertakan `tests/**/*.ts`, jadi `tsc --strict`
juga memeriksa file test.

## Batasan

- Pola `match` bertingkat (`Ok(Some(x))`) ditolak saat typecheck, bukan saat runtime.
- `?` di dalam lambda ditolak typechecker (akan `return` dari lambda, bukan fungsi).
- `json`/`Value` belum ada; `Ordering` belum tersedia di permukaan.
- Implementasi `Map`/`Set` memakai pencarian linear; cukup untuk library logika.
- Builtin hanya tersedia sebagai receiver (`xs.map(…)`), bukan `list.map(xs, …)`.
