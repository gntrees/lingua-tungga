# Lingua Tungga — Spesifikasi Bahasa v0.1

Status: **terkunci untuk M1–M6**. Perubahan setelah M2 dianggap breaking change dan harus lewat versi baru.
Grammar formal: [`grammar.ebnf`](./grammar.ebnf). Rencana proyek & keputusan tingkat tinggi: [`../PLAN.md`](../PLAN.md).

---

## 1. Ruang Lingkup

Lingua Tungga adalah bahasa logika murni yang dikompilasi menjadi library TypeScript, Python, dan Go.

**Yang ada di v1**

- Class (record nominal ber-method) dan enum (sum type)
- Tipe primitif `Int`, `Float`, `Bool`, `Str`; koleksi `List`, `Map`, `Set`; `Option`, `Result`
- Generik
- Template literal, tagged template, JSX (literal tree)
- Import path lokal & URL (dengan lockfile)
- Test class

**Yang tidak ada di v1 dan tidak akan pernah ada**

`null` bebas, exception sebagai alur normal, inheritance, interface user-defined, shared mutable state, tipe `Dynamic`/`any`, pointer, IO, async, concurrency, makro, kode kondisional per-target.

**Yang ditunda**

IO/HTTP/time/random/regex (lewat mekanisme `effectful`), executable/`main`, LSP, playground, target ke-4.

---

## 2. Model Eksekusi

| Aspek | Aturan |
|---|---|
| Evaluasi | Eager, call-by-value. |
| Mutabilitas | Semua nilai **immutable**. `mut` hanya boleh pada variabel lokal (`let mut`), tidak pada field, parameter, atau elemen koleksi. |
| Siklus referensi | Mustahil. Nilai tidak bisa merujuk dirinya sendiri; rekursi hanya lewat struktur terbatas (`Option`, `List`). |
| Global state | Tidak ada. Tidak ada variabel global. |
| Urutan evaluasi | Argumen dievaluasi kiri ke kanan. |
| Short-circuit | `&&` dan `\|\|` short-circuit. |

Konsekuensi: pemetaan ke Rust/Zig nanti tidak membutuhkan GC, dan `Option`/`List` boleh rekursif tanpa takut siklus.

---

## 3. Leksikal

- Encoding: UTF-8. BOM ditolak.
- Komentar: `//` sampai akhir baris, `/* ... */` tidak bersarang.
- Statement dipisah **newline**; titik koma `;` **ditolak**. Ekspresi boleh melanjut ke baris berikutnya hanya jika berada di dalam `(`, `[`, `<`, `{`, atau setelah operator biner.
- Literal angka: desimal, boleh `_` sebagai pemisah (`1_000_000`). Float wajib punya titik (`1.0`). Tidak ada hex/octal/biner di v1.
- Boolean: `true`, `false`.
- `None` adalah kata kunci literal, bukan identifier.
- Escape string: `\n`, `\t`, `\r`, `\\`, `\"`, `` \` ``, `\$`, `\u{...}`. Escape lain = compile error.
- Indentasi tidak bermakna sintaksis.

---

## 4. Penamaan & Formatting

Compiler **menolak** pelanggaran penamaan (bukan warning):

| Entitas | Gaya |
|---|---|
| `class`, `enum`, varian enum | `PascalCase` |
| field, method, parameter, variabel lokal, test fn | `snake_case` |
| nama file | `snake_case.lt` |

Formatter (`lt fmt`) menghasilkan bentuk kanonik:

- Indentasi 2 spasi, tanpa tab.
- Lebar maksimum 100 kolom; container yang melebihi batas dipecah satu item per baris.
- Trailing comma pada argumen/list yang multiline.
- Satu baris kosong antar member class dan antar import; varian enum ditulis rapat.
- Kurung kurawal selalu di baris yang sama dengan header (`fn f() {`).
- Body kosong ditulis `{}`.
- Baris kosong berlebih dikolaps menjadi satu.

`lt fmt` wajib idempoten: `fmt(fmt(x)) == fmt(x)`.

**Batasan sementara:** lexer membuang komentar, jadi `lt fmt` menolak memformat file yang mengandung komentar (dengan pesan jelas) alih-alih menghapusnya diam-diam. Dukungan komentar menunggu desain *trivia attachment* di AST.

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
| `List<T>` | Urut, immutable. Operasi mengembalikan list baru. |
| `Map<K, V>` | Key wajib punya `Eq` + `Ord`. Urutan iterasi = urutan key terurut (`Ord`), **bukan** insertion/hash order — agar output deterministik dan sama di semua target. |
| `Set<T>` | Sama seperti `Map`, elemen terurut. |

### 5.3 Option & Result

```lt
Option<T>   // varian: Some(T) | None
Result<T, E> // varian: Ok(T) | Err(E)
```

Sugar: `T?` = `Option<T>`. `Some`, `None`, `Ok`, `Err` tersedia tanpa import.

### 5.4 Nominal typing

Dua class dengan field identik tetap tipe berbeda. Assign antar keduanya = compile error. Tidak ada structural typing.

### 5.5 Generik

- Hanya pada `class`, `enum`, method, dan tipe.
- Constraint hanya trait bawaan: `Eq`, `Ord`, `Display`. `Map<K, V>` mensyaratkan `K: Eq + Ord`.
- Tanpa higher-kinded types, tanpa associated types, tanpa default type parameter.

---

## 6. Deklarasi

### 6.1 Class

```lt
pub class User {
  name: Str
  age: Int
  address: Address?

  pub fn greet(self) -> Str { `Hi ${self.name}` }

  static fn anonymous() -> User {
    User(name: "?", age: 0, address: None)
  }

  fn normalize(s: Str) -> Str { s.trim().lower() }   // private helper
}
```

Aturan:

- Field **immutable** dan **read-only dari luar**. Tidak ada field privat di v1 (konstruksi named-arg lintas file harus bisa mengisi semua field).
- Konstruksi **wajib named args**, semua field wajib diisi, urutan bebas:
  `User(name: "Ana", age: 30, address: None)`.
- Tanpa inheritance, tanpa `override`, tanpa `abstract`.
- Method dengan `self` = instance method; tanpa `self` = static (wajib murni, tidak boleh menyimpan state). Kata kunci `static` opsional: `fn f()` tanpa `self` sudah static. Static method tidak boleh memakai type parameter class.
- `pub` menentukan bagian dari API. Tanpa `pub` = private ke file.
- Parameter hanya boleh punya tipe nominal/primitive; tidak ada varargs, tidak ada default value di v1.

### 6.2 Enum

```lt
pub enum Role {
  Admin
  Member(Int)
  Guest
}
```

- Varian boleh tanpa payload atau dengan payload (satu atau lebih tipe).
- `enum` tanpa payload sama dengan konstanta bertipe.
- Tidak ada field di enum; perilaku ada di method class yang menerima enum sebagai parameter.

### 6.3 Class generik

```lt
pub class Box<T> {
  value: T
  pub fn get(self) -> T { self.value }
}
```

### 6.4 Test class

```lt
test class UserTest {
  test fn greet_format() {
    let u = User(name: "Ana", age: 30, address: None)
    Expect.equal(u.greet(), "Hi Ana")
  }

  fn helper() -> Int { 1 }   // bukan test
}
```

- `test class` tidak boleh `pub`.
- Hanya `test fn` yang dieksekusi; `fn` biasa = helper.
- `test fn` wajib tanpa parameter dan tanpa return type.

---

## 7. Statement & Ekspresi

**Statement:** `let`, assignment (`x = e`, hanya untuk `mut`), `return`, `if`, `for`, `while`, ekspresi.

**Ekspresi:** literal, identifier, konstruksi class, pemanggilan method, akses field, `[]` list literal, `match`, `if` ekspresi, lambda, template, tagged template, JSX, `?`.

**Lambda:** `fn(x) => expr` atau `fn(x) => { ... }`. Lambda tidak boleh `mut` menangkap variabel luar (tidak ada closure mutable). Lambda tidak boleh mengembalikan `Result` lalu memakai `?` di dalamnya.

**`match`** wajib exhaustive. Pola: `_`, `None`, `Variant`, `Variant(p, ...)`. Tanpa guard, tanpa destructuring class, tanpa pola literal.

```lt
match role {
  Role.Admin     => "admin"
  Role.Member(n) => `member-${n}`
  Role.Guest     => "guest"
}
```

**Loop:** `for x in list` (hanya `List`, bukan `Map`/`Set` — pakai `.keys()`/`.values()`), dan `while cond`. Perubahan state loop hanya lewat `let mut` lokal.

---

## 8. Semantik Operator

| Operator | Aturan |
|---|---|
| `+ - *` (Int) | Wrapping i64. |
| `/` (Int) | Pembagian bulat, **truncate ke arah nol** (`-7 / 2 == -3`). Pembagi 0 = **trap**. |
| `%` (Int) | Sisa, **tanda mengikuti dividend** (`-7 % 2 == -1`). Pembagi 0 = trap. |
| `+ - * /` (Float) | IEEE-754. `/ 0.0` = `±Inf` atau `NaN`, bukan trap. |
| `-` unary (Int) | Wrap: `-Int.min == Int.min`. |
| `== !=` | Nilai. Class/enum: struktural berdasarkan isi. `Float` mengikuti IEEE (`NaN != NaN`). |
| `< <= > >=` | `Int`, `Float`, `Str` (lexicographic per code point). Butuh `Ord`. |
| `&& \|\|` | `Bool`, short-circuit. |
| `!` | `Bool`. |

Catatan pemetaan: Python membulatkan ke bawah untuk `//` dan `%`-nya mengikuti pembagi — emitter **wajib** membungkus operasi ini agar hasilnya identik dengan aturan di atas.

---

## 9. `?` — Propagasi

- `expr?` pada `Option<T>` hanya sah di method yang return `Option<U>`.
- `?` tidak boleh dipakai di dalam lambda: `return`-nya akan keluar dari lambda, bukan dari method.
- `expr?` pada `Result<T, E>` hanya sah di method yang return `Result<U, E>`.
- Tidak boleh campur `Option` dan `Result` dalam satu method di v1.
- `None`/`Err` langsung di-`return`; selain itu mengekstrak nilai.

---

## 10. Trait Bawaan

Tiga trait yang dikenal compiler. Semuanya **di-derive otomatis**:

| Trait | Perilaku default |
|---|---|
| `Display` | Format struktural: `User(name: "Ana", age: 30)`, `Role.Member(3)`. |
| `Eq` | Perbandingan struktural seluruh field/varian. |
| `Ord` | Lexicographic atas field sesuai urutan deklarasi; varian enum diurutkan sesuai urutan deklarasi. |

Penimpaan manual lewat nama method khusus (tanpa sintaks interface):

| Trait | Method override |
|---|---|
| `Display` | `fn to_string(self) -> Str` |
| `Eq` | `fn equals(self, other: User) -> Bool` |
| `Ord` | `fn compare(self, other: User) -> Ordering` |

`Ordering` adalah enum bawaan: `Less`, `Equal`, `Greater`.

---

## 11. Template & Tagged Template

```lt
let s = `Hi ${user.name}, total ${n}`
let q = Sql.query`SELECT * FROM users WHERE id = ${id}`
```

- `${expr}` wajib bertipe yang punya `Display`.
- Hasil template selalu `Str`.
- Multiline **verbatim**: newline dan indentasi di dalam backtick dipertahankan apa adanya. Tidak ada auto-dedent; gunakan `.trim_indent()` dari stdlib.
- Tagged template: `tag` harus path yang menunjuk fungsi/static method (mis. `Sql.query`) dengan signature `(List<Str>, List<Str>) -> T`. Compiler menurunkan menjadi `tag(parts, values)` dengan invariant `parts.len() == values.len() + 1` dan setiap nilai sudah di-`Display`-kan.
- Tidak ada tag bawaan. Tidak ada escaping otomatis.

---

## 12. JSX

```lt
<Element tag="div" class_name="user">
  <Element tag="span">{user.name}</Element>
</Element>
```

- Nama tag = nama class yang terlihat di scope.
- Atribut = named argument. Nilai: string literal atau `{expr}`.
- Atribut bernama kata kunci bahasa (`class`, `for`, `match`, dst.) **dilarang**.
- Children dikumpulkan menjadi argumen `children` **hanya jika** class mendeklarasikan field `children`. Kalau tidak ada field tersebut tapi ada children = compile error.
- Teks polos di antara tag = satu child `Str`. Diperbolehkan hanya jika field `children` bertipe `List<Str>` (atau `List<T>` dengan `T` = `Str`).
- `<X />` = tanpa children.
- Tag pembuka dan penutup wajib sama persis.
- JSX adalah **konstruksi nilai biasa**; tidak ada lifecycle, tidak ada render, tidak ada framework.

---

## 13. Import

```lt
import User from "./models/user.lt"
import Role from "./models/role.lt"
import Post from "https://raw.githubusercontent.com/foo/bar/v1.2.0/src/post.lt"
```

- Yang bisa di-import: `pub class` dan `pub enum`. Tidak ada fungsi bebas.
- Nama di file target harus ada; tidak ada alias, tidak ada wildcard, tidak ada re-export.
- Satu baris per simbol. Dua simbol dari file yang sama = dua baris import.
- Import cycle = compile error, dengan pesan yang menampilkan siklusnya.
- Memakai tipe dari file yang tidak di-import = compile error (tidak ada resolusi implisit).
- File yang di-import **tidak** otomatis mengekspor importnya.

### Lockfile

- **Status saat ini (M2):** hanya import relatif yang jalan; URL remote menghasilkan compile error yang menyebut package manager (M7). File di `vendor/` bisa di-import dengan path relatif.
- Build pertama menulis `lt.lock`: URL, commit SHA, sha256 per file.
- `lt.lock` wajib di-commit. Build berikutnya nol network dan memverifikasi checksum.
- URL menunjuk branch = warning; lock tetap melindungi.
- `lt update` me-refresh. `lt vendor` menyalin ke `vendor/` untuk build offline.

---

## 14. Test

- `lt test -t <target>` meng-generate file test native lalu menjalankannya: `vitest`, `pytest`, `go test`. Untuk TypeScript, `lt test -t ts ... --run` menjalankan `vitest`; tanpa `--run` hanya meng-emit.
- Test class tidak ikut di-emit ke paket library (`lt build`); hanya `lt test` yang menghasilkan file test.
- Assertion hanya lewat class `Expect`: `Expect.equal(a, b)`, `Expect.not_equal`, `Expect.true(x)`, `Expect.false(x)`.
- Tidak ada mocking, tidak ada setup/teardown, tidak ada test paralel di v1. Semua test wajib deterministik dan murni.
- Sebuah target dianggap lulus hanya jika **seluruh** conformance suite lulus di target itu.

---

## 15. Error & Trap

**Compile error** (ditolak compiler): semua pelanggaran di dokumen ini, termasuk penamaan, import cycle, `?` yang salah konteks, `match` tidak exhaustive, tipe tak dikenal.

**Trap** (program berhenti saat runtime, tidak bisa ditangkap dari LT):

- Pembagian/sisa `Int` dengan nol
- Indeks koleksi di luar batas (`List.get` mengembalikan `Option`, jadi tidak trap; akses `xs[i]` tidak ada di v1)
- `unwrap()` pada `None`/`Err`
- Rekursi tak terbatas (stack overflow)

Trap dipetakan ke mekanisme native target (panic/exception) tetapi **tidak pernah** dipakai sebagai alur normal.

---

## 16. Output

- Satu file `.lt` → satu file output, mempertahankan struktur direktori.
- Nama file output tetap `snake_case`. Identitas class/enum tetap `PascalCase`. Method/field diubah sesuai konvensi target oleh emitter (`snake_case` untuk Python/Go, `camelCase` untuk TypeScript).
- Setiap paket output punya satu file runtime inline (`_lingua_rt.ts`, `_lingua_rt.py`, `rt.go`).
- Tabel pemetaan tipe & konstruksi: lihat [PLAN.md §5](../PLAN.md).
- Kode user wajib lulus `tsc --strict`, `ruff` + `mypy --strict`, `go vet` + `gofmt -l`.
- Pemetaan konkret per backend: [`backends/typescript.md`](backends/typescript.md) dan [`backends/python.md`](backends/python.md). Batasan backend v1 (mis. pola `match` bertingkat) didokumentasikan di sana dan selalu muncul sebagai compile error, bukan perbedaan perilaku.

---

## 17. Stdlib v1

| Permukaan | Isi |
|---|---|
| Method `Str` | `len`, `trim`, `trim_indent`, `lower`, `upper`, `split`, `starts_with`, `ends_with`, `contains`, `replace`, `slice`, `chars`, `parse_int`, `parse_float` |
| Method `List<T>` | `len`, `get(Int) -> T?`, `push`, `pop`, `map`, `filter`, `fold`, `any`, `all`, `sort`, `reverse`, `concat`, `contains`, `index_of`, `join(Str) -> Str` (khusus `List<Str>`) |
| Method `Map<K,V>` | `len`, `get(K) -> V?`, `insert`, `remove`, `keys`, `values`, `contains_key` |
| Method `Set<T>` | `len`, `insert`, `remove`, `contains`, `union`, `intersect`, `difference` |
| Method `Option<T>` | `is_some`, `is_none`, `unwrap`, `unwrap_or`, `map`, `and_then` |
| Method `Result<T,E>` | `is_ok`, `is_err`, `unwrap`, `unwrap_or`, `map`, `map_err`, `and_then` |
| Class `Math` | `abs`, `min`, `max`, `floor`, `ceil`, `round`, `sqrt`, `pow`, `pi` — semuanya `Float` (tanpa overloading) |
| Konstruktor `Map.empty()` / `Set.empty()` | masing-masing `Map<K,V>` / `Set<T>`; tipe parameter diambil dari konteks (anotasi `let`, parameter, atau return) |
| Class `Json` | **belum ada di v1**; menunggu enum `Value` dan runtime per target (M7) |
| Class `Expect` | `equal`, `not_equal`, `true`, `false` (hanya boleh dipakai di `test class`) |
| Enum `Value` | **belum ada di v1**; rencananya `Null`, `Bool`, `Int`, `Float`, `Str`, `List`, `Map` |
| Enum `Ordering` | `Less`, `Equal`, `Greater` |

Perubahan stdlib bukan breaking change selama signature lama dipertahankan.
