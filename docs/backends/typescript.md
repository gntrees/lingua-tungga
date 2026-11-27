# Backend TypeScript

Target pertama, dipakai untuk memvalidasi IR dan semantik.

> **Status:** dokumen ini menggambarkan emitter **v1** (`class`). Desain v2 (`package` +
> `struct` + fungsi ber-receiver) belum diimplementasikan; baris bertanda →v2 adalah target,
> bukan perilaku saat ini. Lihat [`../v2-decisions.md`](../v2-decisions.md).

## Representasi

| LT | TypeScript |
|---|---|
| `Int` (i64) | `bigint` + `BigInt.asIntN(64, …)` di setiap operasi |
| `Float` | `number` |
| `Str` | `string`, index/len dihitung per code point (`[...s]`) |
| `Bool` | `boolean` |
| `List<T>` | `lt.LList<T>` (immutable, copy-on-write) |
| `Map<K,V>` | `lt.LMap<K,V>` (entry terurut `Ord`, lookup O(n)) |
| `Set<T>` | `lt.LSet<T>` |
| `Option<T>` | `lt.Opt<T>` = union `{tag:"some"|"none", values, __lt_display}` |
| `Result<T,E>` | `lt.Res<T,E>` = union `{tag:"ok"|"err", …}` |
| `enum` | discriminating union + konstruktor `Enum_Variant(...)`; setiap nilai membawa `__lt_order` (urutan varian) |
| `struct` | `class` dengan field `readonly`, konstruktor objek, `__lt_eq`, `__lt_to_string` |
| fungsi ber-receiver →v2 | method pada `class` tersebut (`p.len()` → `p.len()`) |
| `package` →v2 | satu file modul per package (`src/money.ts`) |
| `Display` | `lt.disp(value)` |
| `Eq` | `lt.eq(a, b)` |
| `Ord` | `lt.cmp(a, b)` |
| `?` pada `Result`/`Option` | `const t = …; if (t.tag === "err"/"none") return t;` |
| template | konkatenasi `+ lt.disp(...)` |
| `to_string()` | `lt.disp(value)` |
| tagged template | pemanggilan fungsi package-level dengan `LList<string>` + `LList<T>`; nilai tidak di-`disp` |
| JSX | `new Struct({ field: …, children: new LList([...]) })` |
| `match` | `switch (value.tag)` |

## Struktur paket

```
out/ts/
  _lingua_rt.ts      runtime inline (di-embed dari runtimes/ts/runtime.ts)
  package.json
  tsconfig.json      strict, ES2022, noEmit
  src/<package>.ts    satu file per package LT →v2
  tests/<package>.test.ts   file `vitest` per package yang punya `test` →v2
```

Import antar modul memakai namespace `m<id>` (`import * as m1 from "./user"`), sehingga tidak ada tabrakan nama.

## Penamaan

- field, method, parameter, variabel lokal: `snake_case` → `camelCase`
- class, enum, varian: tetap `PascalCase`
- kata kunci TypeScript diberi akhiran `_`

## Test

`lt test -t ts <entry>...` meng-emit paket plus `tests/<module>.test.ts`
(`describe`/`it`). `--run` menjalankan `vitest run` di direktori output.
`tsconfig.json` menyertakan `tests/**/*.ts`, jadi `tsc --strict` juga memeriksa
file test.

## Batasan v1

- Pola `match` bertingkat (`Ok(Some(x))`) ditolak saat lowering.
- `?` di dalam lambda ditolak typechecker (akan `return` dari lambda, bukan method).
- `Json`/`Value` belum ada.
- Implementasi `Map`/`Set` memakai pencarian linear; cukup untuk library logika, bukan untuk data besar.
