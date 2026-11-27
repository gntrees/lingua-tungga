# Tutorial: dari nol ke tiga target

Tutorial ini membangun library kecil (katalog buku) dan menjalankannya sebagai paket TypeScript,
Python, dan Go. Contoh lengkapnya ada di [`examples/library`](../examples/library), dan alasan
desain bahasanya ada di [`v2-decisions.md`](v2-decisions.md).

> Dokumen ini mengikuti desain **v2** (`package` + `struct` + fungsi ber-receiver). Korpus
> `examples/` masih v1 sampai fase implementasi selesai; sintaks di bawah adalah target.

## 1. Build compiler

```sh
cargo build --release
export PATH="$PWD/target/release:$PATH"   # binary `lt`
```

Untuk menjalankan target, siapkan toolchain-nya:

| Target | Butuh |
|---|---|
| TypeScript | Node 20+, `npm install` di direktori output (vitest + typescript) |
| Python | Python 3.11+; `pip install pytest mypy` (script conformance membuat venv) |
| Go | Go 1.22+ |

## 2. Tulis model

`book.lt`:

```lt
package book

export enum Genre {
  Fiction
  NonFiction
  Reference
}

export struct Book {
  title: Str
  author: Str
  genre: Genre
  year: Int
}

export fn genre_label(b: Book) -> Str {
  match b.genre {
    Genre.Fiction    => "fiction"
    Genre.NonFiction => "non-fiction"
    Genre.Reference  => "reference"
  }
}

export fn describe(b: Book) -> Str {
  `"${b.title}" by ${b.author} (${b.year}, ${b.genre_label()})`
}
```

Yang perlu diperhatikan:

- Satu file boleh berisi beberapa `package`; di sini satu. Nama package wajib unik dalam satu
  build.
- `struct` hanya data. Perilaku adalah fungsi package-level; `b.genre_label()` hanya sugar untuk
  `genre_label(b)`.
- Semua field immutable. Tidak ada `null`; ketiadaan memakai `Option<T>`.
- `match` wajib exhaustive.
- Plain template literal memakai `Display`; `Int`/`Str`/struct semuanya bisa. Tagged template
  tidak: tag mendeklarasikan `List<T>` dan setiap nilai harus persis `T`. Untuk mengubah nilai
  menjadi `Str`, pakai `to_string()`.

## 3. Tulis logika

`catalog.lt`:

```lt
package catalog

import book { Book }

export struct Catalog {
  books: List<Book>
}

export fn empty() -> Catalog {
  Catalog(books: List.empty())
}

export fn add(c: Catalog, b: Book) -> Catalog {
  Catalog(books: c.books.push(b))
}

export fn size(c: Catalog) -> Int {
  c.books.len()
}

export fn find_by_author(c: Catalog, author: Str) -> List<Book> {
  c.books.filter(fn(b) => b.author == author)
}

export fn newest(c: Catalog) -> Option<Book> {
  let mut best: Option<Book> = None
  for b in c.books {
    best = match best {
      None => Some(b)
      Some(current) => if b.year > current.year { Some(b) } else { best }
    }
  }
  best
}
```

Catatan bahasa:

- Import menunjuk **package**, bukan simbol: `import book { Book }` mengimpor package `book`
  dari file di path itu, lalu mengambil `Book` saja. Tanpa `{ … }`, package diikat sebagai
  qualifier (`book.Book`).
- Koleksi immutable: `push`/`add` mengembalikan nilai baru.
- Fungsi tanpa parameter bertipe struct adalah fungsi biasa; `empty()` dipanggil tanpa receiver.
- Urutan parameter menentukan siapa receiver-nya: `add(c, b)` bisa ditulis `c.add(b)`.
- `mut` hanya untuk variabel lokal; field struct tidak pernah mutable.

## 4. Tulis test

`catalog_test.lt`:

```lt
package catalog_test

import book { Book, Genre }
import catalog { Catalog, add, empty, newest, size }

fn sample() -> Catalog {
  let first = Book(title: "Dune", author: "Frank", genre: Genre.Fiction, year: 1965)
  let second = Book(title: "Code", author: "Ada", genre: Genre.Reference, year: 2020)
  add(add(empty(), first), second)
}

test "adds books" {
  expect.eq(size(sample()), 2)
}

test "newest uses year" {
  expect.eq(newest(sample()).map(fn(b) => b.title), Some("Code"))
}
```

- Test adalah item di dalam package, ditulis `test "judul" { … }`, dan judulnya unik per package.
- `fn sample()` adalah helper biasa; ia private karena tanpa `export`.
- Assertion lewat pseudo-package `expect` (`expect.eq`, `expect.not_equal`, `expect.true`,
  `expect.false`).

## 5. Cek dan format

```sh
lt check examples/library/catalog_test.lt
lt fmt examples/library
```

`lt fmt` mempertahankan komentar. Komentar inline (`x: Int // note`) ditolak; pindahkan ke baris
sendiri.

## 6. Compile dan jalankan

```sh
lt test -t ts examples/library/catalog_test.lt --out out/lib --run
lt test -t py examples/library/catalog_test.lt --out out/lib --run
lt test -t go examples/library/catalog_test.lt --out out/lib --run
```

`--run` memakai runner native: `vitest`, `pytest`, `go test`. Tanpa `--run`, file test saja yang
di-emit.

Untuk hanya membuat library:

```sh
lt build -t ts examples/library/catalog.lt --out out/lib
lt build -t py examples/library/catalog.lt --out out/lib
lt build -t go examples/library/catalog.lt --out out/lib
```

Satu package menjadi satu modul host. Karena contoh di atas berisi dua package (`book` dan
`catalog`) di dua file terpisah, output-nya dua modul:

```
out/ts/src/book.ts, out/ts/src/catalog.ts
out/py/lingua_tungga/book.py, out/py/lingua_tungga/catalog.py
out/go/book/book.go, out/go/catalog/catalog.go
```

Hasil TypeScript kira-kira:

```ts
export class Book {
  readonly title: string;
  readonly author: string;
  readonly genre: Genre;   // enum direpresentasikan sebagai union bertag
  readonly year: bigint;
  // __lt_eq / __lt_cmp / __lt_to_string di-derive
}

export function genre_label(b: Book): string { /* … */ }
```

Nama field/fungsi diubah sesuai konvensi target (`snake_case` → `camelCase` di TypeScript).
Deklarasi tanpa `export` tetap private **di LT**: package lain tidak bisa memanggilnya, termasuk
lewat sugar receiver. Di output ia diberi prefiks `_` (TypeScript/Python) atau huruf kecil (Go)
supaya tetap bisa dipakai file test package itu sendiri tanpa dianggap API publik.

## 7. Import dari GitHub

```lt
import user { User } "https://raw.githubusercontent.com/foo/bar/v1.2.0/src/user.lt"
```

Build pertama menulis `lt.lock` (commit + sha256). Build berikutnya offline. Perintah terkait:

```sh
lt update app.lt   # terima versi baru, tulis ulang lock
lt vendor app.lt   # simpan objek ke vendor/ untuk CI offline
```

Lihat [`package-manager.md`](package-manager.md).

## 8. Yang tidak dilakukan bahasa ini

Tidak ada IO, async, exception, inheritance, `null`, typeclass, atau `main`. Program LT adalah
library murni; efek samping (HTTP, file, UI) tetap di host language.
