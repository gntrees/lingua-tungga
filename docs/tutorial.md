# Tutorial: dari nol ke tiga target

Tutorial ini membangun library kecil (katalog buku) dan menjalankannya sebagai
paket TypeScript, Python, dan Go. Contoh lengkapnya ada di
[`examples/library`](../examples/library).

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
pub enum Genre {
  Fiction
  NonFiction
  Reference
}

pub class Book {
  title: Str
  author: Str
  genre: Genre
  year: Int

  pub fn genre_label(self) -> Str {
    match self.genre {
      Genre.Fiction => "fiction"
      Genre.NonFiction => "non-fiction"
      Genre.Reference => "reference"
    }
  }

  pub fn describe(self) -> Str {
    `"${self.title}" by ${self.author} (${self.year}, ${self.genre_label()})`
  }
}
```

Yang perlu diperhatikan:

- Semua field immutable. Tidak ada `null`; ketiadaan memakai `Option<T>`.
- `match` wajib exhaustive.
- Template literal memakai `Display`; `Int`/`Str`/class semuanya bisa.

## 3. Tulis logika

`catalog.lt`:

```lt
import Book from "./book.lt"

pub class Catalog {
  books: List<Book>

  static fn empty() -> Catalog {
    Catalog(books: [])
  }

  pub fn add(self, book: Book) -> Catalog {
    Catalog(books: self.books.push(book))
  }

  pub fn find_by_author(self, author: Str) -> List<Book> {
    self.books.filter(fn(book) => book.author == author)
  }

  pub fn newest(self) -> Option<Book> {
    let mut best: Option<Book> = None
    for book in self.books {
      best = match best {
        None => Some(book)
        Some(current) => if book.year > current.year { Some(book) } else { best }
      }
    }
    best
  }
}
```

Catatan bahasa:

- Import **per simbol**: kalau butuh `Genre`, import juga dari `book.lt`.
- Koleksi immutable: `push`/`add` mengembalikan nilai baru.
- Method tanpa `self` bersifat static (`Catalog.empty()`).

## 4. Tulis test

`catalog_test.lt`:

```lt
import Book from "./book.lt"
import Genre from "./book.lt"
import Catalog from "./catalog.lt"

test class CatalogTest {
  fn sample() -> Catalog {
    let first = Book(title: "Dune", author: "Frank", genre: Genre.Fiction, year: 1965)
    let second = Book(title: "Code", author: "Ada", genre: Genre.Reference, year: 2020)
    Catalog.empty().add(first).add(second)
  }

  test fn adds_books() {
    Expect.equal(sample().size(), 2)
  }

  test fn newest_uses_year() {
    Expect.equal(sample().newest().map(fn(book) => book.title), Some("Code"))
  }
}
```

`test fn` boleh memanggil helper di test class yang sama.

## 5. Cek dan format

```sh
lt check examples/library/catalog_test.lt
lt fmt examples/library
```

`lt fmt` mempertahankan komentar. Komentar inline (`x: Int // note`) ditolak;
pindahkan ke baris sendiri.

## 6. Compile dan jalankan

```sh
lt test -t ts examples/library/catalog_test.lt --out out/lib --run
lt test -t py examples/library/catalog_test.lt --out out/lib --run
lt test -t go examples/library/catalog_test.lt --out out/lib --run
```

`--run` memakai runner native: `vitest`, `pytest`, `go test`. Tanpa `--run`,
file test saja yang di-emit.

Untuk hanya membuat library:

```sh
lt build -t ts examples/library/catalog.lt --out out/lib
lt build -t py examples/library/catalog.lt --out out/lib
lt build -t go examples/library/catalog.lt --out out/lib
```

Hasil TypeScript kira-kira:

```ts
export class Book {
  readonly title: string;
  readonly author: string;
  readonly genre: Role;   // enum direpresentasikan sebagai union bertag
  readonly year: bigint;
  // ...
}
```

## 7. Import dari GitHub

```lt
import User from "https://raw.githubusercontent.com/foo/bar/v1.2.0/src/user.lt"
```

Build pertama menulis `lt.lock` (commit + sha256). Build berikutnya offline.
Perintah terkait:

```sh
lt update app.lt   # terima versi baru, tulis ulang lock
lt vendor app.lt   # simpan objek ke vendor/ untuk CI offline
```

Lihat [`package-manager.md`](package-manager.md).

## 8. Yang tidak dilakukan bahasa ini

Tidak ada IO, async, exception, inheritance, `null`, atau `main`. Program LT
adalah library murni; efek samping (HTTP, file, UI) tetap di host language.
