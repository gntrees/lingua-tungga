# Lingua Tungga — Spesifikasi Bahasa v2.0

Status: **berlaku** sejak merge v2 ke `main`. Menggantikan v0.1, yang
terkunci untuk M1–M6. Alasan tiap perubahan ada di [`v2-decisions.md`](./v2-decisions.md).
Grammar formal: [`grammar.ebnf`](./grammar.ebnf). Rencana proyek: [`../PLAN.md`](../PLAN.md).

Perubahan terbesar dari v0.1: **`class` dihapus**. Wadah tunggal adalah `package`; isi package
adalah `struct`, `enum`, fungsi, dan binding immutable. Metode menjadi fungsi ber-receiver
eksplisit.

---

## 1. Ruang Lingkup

Lingua Tungga adalah bahasa logika murni yang dikompilasi menjadi library TypeScript, Python,
dan Go.

**Yang ada di v2**

- `package` (namespace nominal, satu file boleh berisi lebih dari satu)
- `struct` (record nominal) dan `enum` (sum type)
- Fungsi package-level, termasuk fungsi ber-receiver (pengganti metode)
- Tipe primitif `Int`, `Float`, `Bool`, `Str`; koleksi `List`, `Map`, `Set`; `Option`, `Result`
- Generik pada `struct`, `enum`, dan fungsi
- Binding level-package (immutable)
- Template literal, tagged template, JSX (literal struct)
- Import package: path lokal & URL, dengan destructuring dan alias
- Deklarasi `test` di dalam package

**Yang tidak ada dan tidak akan pernah ada**

`null` bebas, exception sebagai alur normal, inheritance, interface/trait user-defined,
typeclass, higher-kinded types, shared mutable state, tipe `Dynamic`/`any`, pointer, IO, async,
concurrency, makro, kode kondisional per-target.

**Yang ditunda**

Codegen skema eksternal, executable/`main`, LSP, target ke-4.

---

## 2. Model Eksekusi

| Aspek | Aturan |
|---|---|
| Evaluasi | Eager, call-by-value. |
| Mutabilitas nilai | Semua nilai **immutable**. `mut` hanya boleh pada variabel lokal (`let mut`), tidak pada field, parameter, elemen koleksi, atau binding package. |
| Mutabilitas lokal | Variabel lokal `let mut` boleh di-assign ulang. Ini satu-satunya bentuk mutasi di bahasa. |
| Siklus referensi | Mustahil. Nilai tidak bisa merujuk dirinya sendiri; rekursi hanya lewat struktur terbatas (`Option`, `List`). |
| Global state | Tidak ada global mutable. Binding level-package immutable, dievaluasi eager saat load, dan tidak boleh membentuk siklus inisialisasi. |
| Urutan evaluasi | Argumen dievaluasi kiri ke kanan. |
| Short-circuit | `&&` dan `\|\|` short-circuit. |

Konsekuensi: pemetaan ke Rust/Zig nanti tidak membutuhkan GC, dan `Option`/`List` boleh rekursif
tanpa takut siklus.

---

## 3. Leksikal

- Encoding: UTF-8. BOM ditolak.
- Komentar: `//` sampai akhir baris, `/* ... */` tidak bersarang.
- Statement dipisah **newline**; titik koma `;` **ditolak**. Ekspresi boleh melanjut ke baris
  berikutnya hanya jika berada di dalam `(`, `[`, `<`, `{`, atau setelah operator biner.
- Literal angka: desimal, boleh `_` sebagai pemisah (`1_000_000`). Float wajib punya titik
  (`1.0`). Tidak ada hex/octal/biner.
- Boolean: `true`, `false`.
- `None` adalah kata kunci literal, bukan identifier.
- Escape string: `\n`, `\t`, `\r`, `\\`, `\"`, `` \` ``, `\$`, `\u{...}`. Escape lain =
  compile error.
- Indentasi tidak bermakna sintaksis.

**Kata kunci v2:** `package`, `export`, `struct`, `enum`, `fn`, `test`, `let`, `mut`,
`return`, `if`, `else`, `for`, `in`, `while`, `match`, `import`, `from`, `as`, `true`,
`false`, `None`, `_`.

**Kata kunci v1 yang dihapus tetap dicadangkan:** `class`, `pub`, `static`, `self`. Memakainya
menghasilkan error yang menyebut penggantinya (`class` → `package` + `struct`, `pub` → `export`,
`self` → parameter receiver eksplisit).

**Soft keyword.** Sebuah kata kunci boleh menjadi *nama* bila posisinya sudah pasti nama:

- Setelah `.`, **semua** kata kunci boleh: `b.from`, `b.match`, `b.class`.
- Nama field, label named argument, parameter, dan atribut JSX boleh **semua** kata kunci, karena
  posisinya selalu diikuti `:` (atau `=`): `struct Range { from: Int }`, `Range(from: 1)`.
- Nama fungsi, `let`, binding pola, variabel `for`, assignment target, nama package, serta item
  dan alias import hanya boleh memakai `from`, `as`, dan `in` — satu-satunya kata kunci yang
  tidak bisa memulai statement, ekspresi, tipe, atau item. Jadi `fn from(r: Range)` dan
  `r.from()` sah, sedangkan `fn match(...)` tidak.
- Nama tipe/struct/enum/varian tidak berubah: wajib `PascalCase`, dan satu-satunya kata kunci
  kapital adalah `None`.

Memakai kata kunci terlarang di posisi nama menghasilkan error yang menyarankan akhiran `_`
(mis. `` `match` is a keyword; rename it to `match_` ``). Emitter host meng-angle nama itu bila
bahasa target membutuhkannya: field `from` menjadi `from` di TypeScript, `from_` di Python, dan
`From` di Go.

---

## 4. Penamaan & Formatting

Compiler **menolak** pelanggaran penamaan (bukan warning). Aturan ini ditegakkan, bukan sekadar
didokumentasikan.

| Entitas | Gaya |
|---|---|
| `struct`, `enum`, varian enum | `PascalCase` |
| fungsi, field, parameter, variabel lokal, binding package | `snake_case` |
| nama package | `lowercase_snake_case` (`[a-z][a-z0-9_]*`) |
| nama file | `snake_case.lt` |
| judul test | string literal bebas; tidak ada aturan gaya |

**Keunikan case-insensitive:** dalam satu package, dua nama tidak boleh sama tanpa memandang
huruf besar/kecil (`Format` dan `format` tidak boleh hidup berdampingan). Alasannya pemetaan ke
Go: fungsi `format` menjadi `Format`, yang akan bertabrakan dengan `struct Format`.

Formatter (`lt fmt`) menghasilkan bentuk kanonik:

- Indentasi 2 spasi, tanpa tab.
- Lebar maksimum 100 kolom; container yang melebihi batas dipecah satu item per baris.
- Trailing comma pada argumen/list yang multiline.
- Satu baris kosong antar item package; varian enum ditulis rapat.
- Import diurutkan berdasarkan path. Beberapa entri `import` yang berdampingan
  membentuk satu blok yang diurutkan; baris kosong memisahkan sub-blok dan
  masing-masing diurutkan sendiri. Entri di dalam `import ( … )` tetap berada di
  bloknya, satu per baris.
- Beberapa blok `package` dalam satu file dipisah **satu baris kosong**, dan urutannya
  dipertahankan seperti di sumber (formatter tidak mengurutkan ulang package).
- Kurung kurawal selalu di baris yang sama dengan header (`fn f() {`).
- Body kosong ditulis `{}`.
- Baris kosong berlebih dikolaps menjadi satu.

`lt fmt` wajib idempoten: `fmt(fmt(x)) == fmt(x)`.

**Komentar:** `lt fmt` mempertahankan komentar dan idempoten dengan komentar.

- Komentar `//` dan `/* ... */` ditempelkan ke konstruk berikutnya di dalam body yang sama
  (file, package, block, atau `match`). Komentar setelah konstruk terakhir dicetak sebelum `}`
  penutup.
- Komentar **inline** (berbagi baris dengan kode, mis. `x: Int // note`) ditolak dengan pesan
  jelas; pindahkan ke baris sendiri.
- `/* ... */` dinormalkan ke bentuk `/*`, ` * ...`, ` */`. `///` dipertahankan.
- Komentar di dalam ekspresi (mis. di antara argumen) ikut menempel ke konstruk berikutnya/akhir
  body; urutannya dipertahankan tetapi posisi horizontal bisa berpindah.

---

## 5. Tipe

### 5.1 Primitif

| Tipe | Semantik |
|---|---|
| `Int` | i64, two's complement, **overflow wrapping**. |
| `Float` | f64 IEEE-754. `NaN` ada; `NaN != NaN`. |
| `Bool` | `true`/`false`. |
| `Str` | Immutable, urutan **Unicode code point** (bukan byte, bukan grapheme cluster). `len()` = jumlah code point. |

Tidak ada konversi implisit `Int` ↔ `Float`. `1 + 1.0` = compile error.

### 5.2 Koleksi

| Tipe | Catatan |
|---|---|
| `List<T>` | Urut, immutable. Operasi mengembalikan list baru. Literal: `[1, 2]`. |
| `Map<K, V>` | Key wajib punya `Eq` + `Ord`. Urutan iterasi = urutan key terurut (`Ord`), **bukan** insertion/hash order — agar output deterministik dan sama di semua target. Tidak punya literal; bangun dari `map.empty()`. Pencarian key memakai binary search dan duplikat dibuang dalam satu lintasan setelah sortir. |
| `Dict<V>` | Key selalu `Str`, urutan iterasi = **urutan insertion** (`Dict` tidak punya `Ord` yang terlihat; `cmp`-nya kanonik agar `Eq ⇔ cmp == 0`). Literal: `["a": 1, "b": 2]`, kosong `[:]`. |
| `Set<T>` | Sama seperti `Map`, elemen terurut. |

`Dict` tidak peduli urutan saat dibandingkan (`["a": 1, "b": 2]` = `["b": 2, "a": 1]`), tetapi
`Display`-nya mengikuti urutan penulisan. `keys()`/`values()` juga mengikuti urutan itu.
Meng-`insert` key yang sudah ada mempertahankan posisinya dan menimpa nilainya.

### 5.3 Option & Result

```lt
Option<T>    // varian: Some(T) | None
Result<T, E> // varian: Ok(T) | Err(E)
```

Sugar: `T?` = `Option<T>`. `Some`, `None`, `Ok`, `Err` tersedia tanpa import.

### 5.4 Nominal typing

Dua `struct` dengan field identik tetap tipe berbeda. Demikian pula `struct` dan `enum`. Assign
antar keduanya = compile error. Tidak ada structural typing.

Identitas nominal adalah pasangan **(package, nama)**, bukan sekadar nama: `money.Price` dan
`user.Price` adalah dua tipe berbeda.

### 5.5 Generik

- Hanya pada `struct`, `enum`, dan fungsi.
- Constraint hanya trait bawaan: `Eq`, `Ord`, `Display`. `Map<K, V>` mensyaratkan
  `K: Eq + Ord`.
- Tanpa higher-kinded types, tanpa associated types, tanpa default type parameter.

---

## 6. Deklarasi

### 6.1 Package

```lt
// src/money.lt
package money

export struct Price {
  amount: Int
  currency: Str
}

export fn format(p: Price) -> Str {
  `${p.amount / 100}.${p.amount % 100} ${p.currency}`
}

fn helper() -> Int { 0 }   // private: tidak bisa di-import package lain
```

Aturan:

- `package` **wajib** membungkus semua deklarasi. Tidak ada deklarasi telanjang di top-level
  file, dan tidak ada `package` bersarang.
- Satu file boleh berisi 1..N blok `package`; tiap blok independen.
- Package bersifat **file-scoped**: dua file yang sama-sama menulis `package money` adalah dua
  deklarasi berbeda, dan karena nama wajib unik per build, itu **compile error**.
- Nama package harus unik dalam satu build (seluruh graf import). Identitas tipe adalah
  `(package, nama)`.
- Isi package: `struct`, `enum`, fungsi, binding `let`, dan `test`. Tidak ada kelas, tidak ada
  method di dalam tipe.
- Visibilitas ditentukan `export` per-deklarasi; default private (hanya terlihat di package itu).
  Aturan ini berlaku untuk **semua** jalan pemanggilan: nama polos, path berkualifikasi, dan sugar
  receiver `p.f()` — fungsi private tidak bisa dipanggil dari package lain lewat jalur mana pun.
- Binding level-package immutable, dievaluasi eager, dan tidak boleh membentuk siklus.

### 6.2 Struct

```lt
export struct User {
  name: Str
  age: Int
  address: Address?
}
```

- Field immutable dan read-only dari luar. Tidak ada field privat: konstruksi named-arg lintas
  package harus bisa mengisi semua field.
- Konstruksi **wajib named args**, semua field wajib diisi, urutan bebas:
  `User(name: "Ana", age: 30, address: None)`.
- Tanpa constructor, tanpa inheritance, tanpa `override`, tanpa `abstract`.
- `struct` kosong sah (`export struct Unit {}`), berguna sebagai penanda.
- Generik menempel di struct: `export struct Box<T> { value: T }`.

### 6.3 Fungsi & receiver

```lt
export fn empty() -> Catalog { Catalog(books: List.empty()) }

export fn add(c: Catalog, b: Book) -> Catalog {
  Catalog(books: c.books.push(b))
}

export fn size(c: Catalog) -> Int { c.books.len() }
```

`c.add(b)` ≡ `add(c, b)`; `c.size()` ≡ `size(c)`.

Aturan:

- Fungsi ber-receiver adalah fungsi biasa yang **parameter pertamanya** bertipe `struct`/`enum`.
  Tidak ada penanda khusus, tidak ada `self`.
- Sugar `nilai.f(args)` hanya berlaku bila fungsi dengan nama `f` ada di package tempat tipe
  nilai itu dideklarasikan. Tidak ada dispatch dinamis; hasilnya selalu `f(nilai, args)`.
- Fungsi ber-receiver **wajib** berada di package yang sama dengan tipe receiver-nya
  (coherence). Ini juga yang membuat pemetaan ke method Go sah.
- Overloading hanya sah bila **tipe receiver** berbeda: `fn len(p: Point)` dan `fn len(s: Shape)`
  boleh berdampingan. Overloading berdasarkan nama atau arity dilarang.
- Parameter hanya boleh bertipe nominal/primitif; tidak ada varargs, tidak ada default value.
- Fungsi tanpa parameter bertipe struct adalah fungsi biasa (`catalog.empty()` hanya jika
  `empty` menerima receiver; kalau tidak, panggil `empty()`).

### 6.4 Enum

```lt
export enum Role {
  Admin
  Member(Int)
  Guest
}
```

- Varian boleh tanpa payload atau dengan payload (satu atau lebih tipe).
- `enum` tanpa payload sama dengan konstanta bertipe.
- Tidak ada field di enum; perilaku ada di fungsi package-level yang menerima enum sebagai
  parameter, dan boleh memakai sugar receiver (`role.label()` ≡ `label(role)`).
- Varian dirujuk sebagai `Role.Member(3)`; dari package lain dengan qualifier
  (`models.Role.Member(3)`) atau lewat destructuring.

### 6.5 Binding level-package

```lt
package money

export let default_currency: Str = "IDR"
let lookup: Map<Str, Int> = map.empty()
```

- Immutable, eager, tanpa parameter.
- Boleh merujuk binding lain di package yang sama selama tidak membentuk siklus.
- Diekspos lewat `export` dan diakses sebagai `money.default_currency` atau lewat destructuring.

### 6.6 Test

```lt
package money

test "formats with currency" {
  let p = Price(amount: 1250, currency: "IDR")
  expect.eq(format(p), "12.50 IDR")
}
```

- `test "judul" { ... }` adalah item package; judul unik per package.
- Tidak diemit ke paket library (`lt build`); hanya `lt test` yang menghasilkan file test.
- Body test adalah blok statement biasa; tidak ada parameter, tidak ada return type.
- Test boleh memakai deklarasi apa pun di package-nya, termasuk yang private.

---

## 7. Statement & Ekspresi

**Statement:** `let`, assignment (hanya untuk `mut`), `return`, `if`, `for`, `while`, ekspresi.

**Ekspresi:** literal, identifier, konstruksi struct, pemanggilan fungsi, pemanggilan receiver,
akses field, path berkualifikasi, `[]` list literal, `[:]`/`["k": v]` dict literal, `match`,
`if` ekspresi, lambda, template,
tagged template, JSX, `?`.

**Blok:** hanya badan `fn`, cabang `if`, dan cabang `match` yang bernilai dari ekspresi
terakhirnya. Blok lepas adalah statement-only; `let x = { … }` ilegal.

**Lambda:** `fn(x) => expr` atau `fn(x) => { ... }`. Lambda tidak boleh `mut` menangkap variabel
luar (tidak ada closure mutable). Lambda tidak boleh mengembalikan `Result` lalu memakai `?` di
dalamnya.

**`match`** wajib exhaustive. Pola: `_`, `None`, `Variant`, `Variant(p, ...)`. Tanpa guard, tanpa
destructuring struct, tanpa pola literal. Pola bertingkat (mis. `Ok(Some(x))`) ditolak compiler.

```lt
match role {
  Role.Admin     => "admin"
  Role.Member(n) => `member-${n}`
  Role.Guest     => "guest"
}
```

**Loop:** `for x in list` (hanya `List`, bukan `Map`/`Set` — pakai `.keys()`/`.values()`), dan
`while cond`. Perubahan state loop hanya lewat `let mut` lokal.

**Path berkualifikasi:** maksimum 3 segmen.

| Segmen | Bentuk | Contoh |
|---|---|---|
| 2 | `package.{tipe\|fungsi\|binding}` | `money.Price`, `money.format(p)`, `money.default_currency` |
| 3 | `package.Type.Variant` | `models.Role.Admin` |
| 4+ | tidak ada | `money.Price.field` = compile error |

Resolusi segmen pertama: **binding lokal** → **nama hasil destructuring** → **qualifier
package dari `import`**. Bila sebuah nama cocok di lebih dari satu kategori = error, bukan
resolusi diam-diam.

---

## 8. Semantik Operator

| Operator | Aturan |
|---|---|
| `+ - *` (Int) | Wrapping i64. |
| `/` (Int) | Pembagian bulat, **truncate ke arah nol** (`-7 / 2 == -3`). Pembagi 0 = **trap**. |
| `%` (Int) | Sisa, **tanda mengikuti dividend** (`-7 % 2 == -1`). Pembagi 0 = trap. |
| `+ - * /` (Float) | IEEE-754. `/ 0.0` = `±Inf` atau `NaN`, bukan trap. |
| `-` unary (Int) | Wrap: `-Int.min == Int.min`. |
| `== !=` | Nilai. Struct/enum: struktural berdasarkan isi. `Float` mengikuti IEEE (`NaN != NaN`). |
| `< <= > >=` | `Int`, `Float`, `Str` (lexicographic per code point). Butuh `Ord`. |
| `&& \|\|` | `Bool`, short-circuit. |
| `!` | `Bool`. |

Catatan pemetaan: Python membulatkan ke bawah untuk `//` dan `%`-nya mengikuti pembagi — emitter
**wajib** membungkus operasi ini agar hasilnya identik dengan aturan di atas.

---

## 9. `?` — Propagasi

- `expr?` pada `Option<T>` hanya sah di fungsi yang return `Option<U>`.
- `?` tidak boleh dipakai di dalam lambda: `return`-nya akan keluar dari lambda, bukan dari
  fungsi.
- `expr?` pada `Result<T, E>` hanya sah di fungsi yang return `Result<U, E>`.
- Tidak boleh campur `Option` dan `Result` dalam satu fungsi.
- `None`/`Err` langsung di-`return`; selain itu mengekstrak nilai.

---

## 10. Trait Bawaan

Tiga trait yang dikenal compiler. Semuanya **di-derive otomatis** untuk setiap `struct` dan
`enum`:

| Trait | Perilaku default |
|---|---|
| `Display` | Format struktural: `User(name: "Ana", age: 30)`, `Role.Member(3)`. |
| `Eq` | Perbandingan struktural seluruh field/varian. |
| `Ord` | Lexicographic atas field sesuai urutan deklarasi; varian enum diurutkan sesuai urutan deklarasi. |

- **`Eq` dan `Ord` tidak bisa ditimpa.** Keduanya dipakai `sort`, `Map`, dan `Set`, dan ketiga
  target menerapkannya lewat jalur berbeda (`sort.Slice`, `__lt__`, comparator); membiarkannya
  ditimpa akan memecah paritas.
- **`Display` bisa ditimpa** dengan mendeklarasikan fungsi ber-receiver bernama `to_string` di
  package tempat tipe itu dideklarasikan:
  `export fn to_string(p: Price) -> Str { … }`.
- Setiap nilai yang punya `Display` bisa dikonversi lewat `to_string()`. Fungsi `to_string`
  milik user menang atas versi bawaan, **termasuk di dalam interpolasi `${…}`**.
- `Ordering` dipakai secara internal oleh runtime (`sort`, `Map`, `Set`) tetapi **belum tersedia di
  permukaan bahasa**: `Ordering` ditolak sebagai tipe dan `Ordering.Less` bukan nilai. Ini keputusan
  sadar sampai ada kebutuhan nyata dan konstanta di ketiga runtime.

Ord dipakai oleh `sort`, `Map`, dan `Set`:
- struct: lexicographic atas field sesuai urutan deklarasi (dibandingkan rekursif dengan `Ord`).
- enum: sesuai urutan deklarasi varian, lalu payload.
- `Option`/`Result`: urutan internal tetap (`none < some`, `ok < err`) untuk stabilitas, bukan
  bagian dari kontrak publik.

---

## 11. Template & Tagged Template

```lt
let s = `Hi ${user.name}, total ${n}`

enum SqlArg {
  I(Int)
  S(Str)
}

let q = query`SELECT * FROM users WHERE id = ${SqlArg.I(id)}`
```

- Pada **plain template**, `${expr}` wajib bertipe yang punya `Display`, dan hasilnya selalu
  `Str`. `Unit` tidak boleh diinterpolasi. `Display` yang dipakai adalah versi turunan atau
  versi `to_string` milik user bila ada.
- Multiline **verbatim**: newline dan indentasi di dalam backtick dipertahankan apa adanya.
  Tidak ada auto-dedent; gunakan `.trim_indent()` dari stdlib.
- **Tagged template**: tag harus path yang menunjuk **fungsi package-level** (mis. `query`,
  `sql.query`), dengan signature `(List<Str>, List<T>) -> R`. `T` ditulis konkret dan tidak
  boleh memakai type parameter.
- Setiap `${expr}` pada tagged template harus **persis** bertipe `T`; tidak ada konversi
  `Display`, tidak ada pembungkusan `Some` implisit, tidak ada pemilihan varian otomatis. Nilai
  diteruskan apa adanya, sehingga tag dapat memeriksanya lewat `match`.
- Nilai bertipe campuran dinyatakan dengan enum milik user yang dipakai sebagai `T`.
- Invariant tetap `parts.len() == values.len() + 1`. Tidak ada tag bawaan dan tidak ada escaping
  otomatis.
- Untuk mengubah nilai menjadi `Str` di posisi `${…}` yang bertipe `T = Str`, gunakan
  `to_string()`.

---

## 12. JSX

```lt
<Element tag="div" class_name="user">
  <Element tag="span">{user.name}</Element>
</Element>
```

- Nama tag = nama `struct` yang terlihat di scope (lokal, hasil destructuring, atau qualifier
  package). **Tag huruf kecil = compile error**, karena nama tipe wajib `PascalCase` dan Go
  hanya mengekspor identifier berhuruf besar.
- Atribut = named argument. Nilai: string literal atau `{expr}`.
- Atribut boleh bernama kata kunci, karena posisinya selalu diikuti `=` (`<div class="x">`).
  Nama atribut yang bukan field struct tetap compile error seperti named argument biasa.
- Children dikumpulkan menjadi argumen `children` **hanya jika** struct mendeklarasikan field
  `children`. Kalau tidak ada field tersebut tetapi ada children = compile error.
- Teks polos di antara tag = satu child `Str`. Diperbolehkan hanya jika field `children` bertipe
  `List<Str>` (atau `List<T>` dengan `T` = `Str`).
- `<X />` = tanpa children.
- Tag pembuka dan penutup wajib sama persis.
- JSX adalah **konstruksi nilai biasa**: `<Element …>` ≡ `Element(…)`. Tidak ada lifecycle,
  tidak ada render, tidak ada framework.

---

## 13. Import

```lt
// alternatif 1 — destructuring
import money { Price, format } "./wallet/money.lt"

// alternatif 2 — qualifier (money.Price, money.format(...))
import money "./wallet/money.lt"

// alternatif 3 — alias
import money as cash { Price as P } "./wallet/money.lt"

// remote
import post "https://raw.githubusercontent.com/foo/bar/v1.2.0/src/post.lt"

// grup: satu entri per baris, tanpa koma
import (
  money { Price }
  post  "https://raw.githubusercontent.com/foo/bar/v1.2.0/src/post.lt"
)
```

- Yang bisa di-import **hanya package**, bukan file dan bukan simbol lepas. Path wajib; nama
  package ditulis eksplisit dan diverifikasi benar-benar ada di file target.
- Nama package harus unik dalam satu build. Import cycle = compile error, dengan pesan yang
  menampilkan siklusnya.
- Satu entri `import` menunjuk satu package. Satu file berisi tiga package = tiga entri import.
- **Destructuring** (`{ … }`) opsional dan hanya bisa menyebut deklarasi ber-`export`. Bila
  dihilangkan, package diikat sebagai qualifier. Destructuring **tidak** mematikan qualifier:
  `import money { Price }` tetap boleh memakai `money.format(...)`. Daftar kosong `{}` = error.
- **Alias** (`as`) diizinkan pada qualifier maupun pada nama hasil destructuring.
- **Grup** `import ( … )` setara dengan menulis entri satu per satu; hanya pengelompokan, tanpa
  arti tambahan di level semantik. `import . "path"` (dot import) dan `import _ "path"` (blank
  import) ditolak: setiap nama selalu dirujuk lewat qualifier, dan package hanya dimuat bila ada
  nama yang dipakai.
- Kata kunci `from` sudah tidak dipakai di import; penulisannya menghasilkan error yang
  menyarankan bentuk baru.
- Nama import yang bentrok dengan deklarasi lokal, atau sesama nama import, = compile error
  (tidak ada aturan "lokal menang").
- Memakai tipe/fungsi dari package yang tidak di-import = compile error (tidak ada resolusi
  implisit).
- File yang di-import **tidak** otomatis mengekspor importnya (tidak ada re-export).

### Lockfile

- Build pertama menulis `lt.lock`: URL, canonical URL, commit SHA (bila GitHub), sha256 per
  file. Format `lt.lock` adalah TOML (`[[package]]`).
- `lt.lock` wajib di-commit. Build berikutnya memakai `vendor/<sha256>` atau cache
  `objects/<sha256>` tanpa jaringan, dan memverifikasi checksum setiap kali.
- URL menunjuk branch = warning konseptual; untuk `raw.githubusercontent.com` ref di-resolve ke
  commit SHA sehingga `canonical` tidak bisa berubah.
- `lt update` me-refresh lock. `lt vendor` menyalin objek ke `vendor/` untuk build offline.
- Cache default `$XDG_CACHE_HOME/lt` atau `~/.cache/lt`; `LT_CACHE_DIR` menimpanya.
- Detail perilaku dan batasan: [`package-manager.md`](package-manager.md).

---

## 14. Test

- `lt test -t <target>` meng-generate file test native lalu menjalankannya: `vitest`, `pytest`,
  `go test`. Untuk TypeScript, `lt test -t ts ... --run` menjalankan `vitest`; tanpa `--run`
  hanya meng-emit.
- Deklarasi `test` tidak ikut di-emit ke paket library (`lt build`); hanya `lt test` yang
  menghasilkan file test.
- Judul test unik per package. Emitter memetakan judul ke nama fungsi native dan **wajib**
  mendeteksi tabrakan hasil mangling.
- Assertion hanya lewat pseudo-package `expect`: `expect.eq(a, b)`, `expect.not_equal(a, b)`,
  `expect.true(x)`, `expect.false(x)`.
- Tidak ada mocking, tidak ada setup/teardown, tidak ada test paralel. Semua test wajib
  deterministik dan murni.
- Sebuah target dianggap lulus hanya jika **seluruh** conformance suite lulus di target itu.

---

## 15. Error & Trap

**Compile error** (ditolak compiler): semua pelanggaran di dokumen ini, termasuk penamaan,
package dengan nama ganda, import cycle, `?` yang salah konteks, `match` tidak exhaustive, tipe
tak dikenal, path lebih dari 3 segmen, tag JSX huruf kecil.

**Trap** (program berhenti saat runtime, tidak bisa ditangkap dari LT):

- Pembagian/sisa `Int` dengan nol
- Indeks koleksi di luar batas (`List.get` mengembalikan `Option`, jadi tidak trap)
- `unwrap()` pada `None`/`Err`
- Rekursi tak terbatas (stack overflow); kedalaman maksimum berbeda per host dan tidak dijamin
  seragam

Trap dipetakan ke mekanisme native target (panic/exception) tetapi **tidak pernah** dipakai
sebagai alur normal.

---

## 16. Output

- **Satu package → satu modul host.** Satu file `.lt` berisi tiga package menghasilkan tiga
  modul output di direktori file sumbernya:
  `out/ts/src/<dir>/money.ts`, `out/py/lingua_tungga/<dir>/money.py`,
  `out/go/<dir>/money/`.
- Nama modul output ditentukan nama package, bukan nama file `.lt`.
- Nama file output tetap `snake_case`. Identitas `struct`/`enum` tetap `PascalCase`. Nama
  fungsi/field diubah sesuai konvensi target oleh emitter (`snake_case` untuk Python/Go,
  `camelCase` untuk TypeScript), dengan `export` dipetakan ke `export` (TS), kapitalisasi (Go),
  dan prefiks `_` untuk yang private (Python).
- Setiap paket output punya satu file runtime inline (`_lingua_rt.ts`, `rt.py`, `rt.go`).
- Tabel pemetaan tipe & konstruksi: lihat [PLAN.md §5](../PLAN.md).
- Kode user wajib lulus `tsc --strict`, `mypy --strict`, serta `go vet` dan `gofmt -l` kosong.
  Formatter kanonik: `lt fmt`.
- Pemetaan konkret per backend: [`backends/typescript.md`](backends/typescript.md),
  [`backends/python.md`](backends/python.md), [`backends/go.md`](backends/go.md).

---

## 17. Stdlib v2

Pseudo-class v1 menjadi **pseudo-package** huruf kecil, konsisten dengan aturan penamaan
package. Semuanya tersedia tanpa `import`.

| Permukaan | Isi |
|---|---|
| Fungsi universal | `to_string() -> Str` untuk setiap nilai ber-`Display`; `to_string` milik user menang, termasuk di interpolasi |
| `str` | `len`, `trim`, `trim_indent`, `lower`, `upper`, `split`, `starts_with`, `ends_with`, `contains`, `replace`, `slice`, `chars`, `parse_int`, `parse_float` |
| `list` | `len`, `get(Int) -> T?`, `push`, `pop`, `map`, `filter`, `fold`, `any`, `all`, `sort`, `reverse`, `concat`, `contains`, `index_of`, `join(Str) -> Str` (khusus `List<Str>`) |
| `map` | `empty()`, `len`, `get(K) -> V?`, `insert`, `remove`, `keys`, `values`, `contains_key` |
| `dict` | `len`, `get(Str) -> V?`, `insert(Str, V)`, `remove(Str)`, `keys() -> List<Str>`, `values() -> List<V>`, `contains_key(Str)`; dibangun dari literal `["k": v]` |
| `set` | `empty()`, `len`, `insert`, `remove`, `contains`, `union`, `intersect`, `difference` |
| `option` | `is_some`, `is_none`, `unwrap`, `unwrap_or`, `map`, `and_then` |
| `result` | `is_ok`, `is_err`, `unwrap`, `unwrap_or`, `map`, `map_err`, `and_then` |
| `math` | `abs`, `min`, `max`, `floor`, `ceil`, `round`, `sqrt`, `pow`, `pi` — semuanya `Float` |
| `json` | **belum ada**; menunggu enum `Value` dan runtime per target |
| `expect` | `eq`, `not_equal`, `true`, `false` (hanya sah di dalam `test`) |
| Enum bawaan | `Ordering` (`Less`, `Equal`, `Greater`); `Value` belum ada |

Cara pakai: fungsi bawaan dipanggil **hanya** lewat sugar receiver (`xs.map(…)`, `s.len()`,
`m.get(k)`, `o.is_some()`). Bentuk fungsi berkualifikasi (`list.map(xs, …)`, `str.len(s)`) **belum
didukung**; yang tersedia sebagai path hanya `map.empty()`, `set.empty()`, `math.*`, dan `expect.*`.
Nama pseudo-package adalah kata kunci tercadang: mendeklarasikan `package str { … }` sendiri =
compile error.

Perubahan stdlib bukan breaking change selama signature lama dipertahankan.
