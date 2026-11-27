# LT v2 — Ledger Keputusan: `class` → `package`

Status: **draft eksplorasi**, dikunci di branch `explore/package-struct`.
Dokumen ini adalah sumber kebenaran selama eksplorasi; `PLAN.md`, `docs/spec.md`, dan
`docs/grammar.ebnf` belum disesuaikan (lihat §12).

Yang berubah dalam satu kalimat: `class` dihapus; wadah tunggal jadi `package`, isi package
adalah `struct`, `enum`, `fn`, dan binding immutable; metode menjadi fungsi ber-receiver
eksplisit.

---

## 1. Positioning & batas

| # | Keputusan | Alasan |
|---|---|---|
| 1.1 | Tetap transpiler 3 target (TS/Py/Go) dengan paritas perilaku sebagai invarian keras. | Seluruh nilai repo (3 emitter, conformance lintas target, aturan "tak bisa dipetakan = compile error") berdiri di atas premis ini. |
| 1.2 | Diferensiasi resmi: paritas 3 target + output readable & lulus checker host + templating sebagai sintaks inti + nominal tanpa typeclass/HKT. | Ini tiga sumbu yang membedakan dari Elm/PureScript/Haskell *dan* dari protobuf. Di sumbu "bahasa fungsional murni" LT memang kalah. |
| 1.3 | 20 target diperlakukan sebagai hipotesis, bukan lingkup. Ditambahkan "Uji Kelayakan 20 Target" (5 pertanyaan per fitur baru). | Menjaga sumber tetap sempit tanpa menjanjikan pekerjaan emitter yang belum ada. |
| 1.4 | LT = **pure core portabel**; host = shell ber-efek. Nol IO, nol async, selamanya. | Efek adalah satu-satunya hal yang tak bisa dipetakan seragam ke banyak host. Begitu LT punya efek, ia berhenti portabel dan menjadi Haxe. |
| 1.5 | Codegen skema (protobuf/SQL → `struct` LT) **bukan** fitur LT sekarang; dicatat sebagai arah. | Mengubah LT dari bahasa menjadi platform codegen; keputusan positioning, bukan detail. |

### Uji Kelayakan 20 Target (wajib dijawab setiap fitur baru)

1. Runtime host seperti apa yang dibutuhkan, dan berapa baris per target?
2. Bagaimana `Eq`/`Ord`/`Display`-nya didefinisikan?
3. Apa perilakunya di target tanpa generik (C)?
4. Apakah aritmetika/urutan/format-nya terdefinisi penuh (bukan warisan host)?
5. Apakah pemetaannya nominal 1:1?

---

## 2. Struktur program

| # | Keputusan | Alasan |
|---|---|---|
| 2.1 | `class` dihapus. Wadah tunggal: `package nama { ... }`. | `class` memikul 5 peran sekaligus (tipe record, wadah fungsi, namespace statis, tag JSX, tag tagged-template); memisahkannya membuat tiap peran punya rumah yang benar. |
| 2.2 | `package` wajib membungkus semua deklarasi. Tidak ada deklarasi telanjang di top-level file. Tidak ada nesting package. | Menghindari dua sistem visibilitas dan nama package karangan untuk deklarasi telanjang. |
| 2.3 | Satu file mengandung 1..N blok `package`. | Permintaan awal; juga memisahkan "file" (unit editor) dari "package" (unit semantik). |
| 2.4 | Package bersifat **nominal & file-scoped**: dua file yang sama-sama mendeklarasikan `package money` adalah dua package berbeda → error karena nama wajib unik. Tidak ada merge lintas file. | Merge-alias-Go bertentangan langsung dengan multi-package-per-file. |
| 2.5 | Nama package **unik per build**, pola `[a-z][a-z0-9_]*`, tanpa titik. | Membuat identitas nominal `(package, nama)` tak ambigu. |
| 2.6 | Satu package = satu modul host: `out/ts/<dir>/money.ts`, `out/py/<dir>/money.py`, `out/go/<dir>/money/`. Satu file `.lt` berisi 3 package → 3 modul output. | Satu-satunya bentuk yang bisa dipetakan ke Go (satu package per direktori). |
| 2.7 | Secara internal, **satu `ModuleId` per package**, bukan per file. Loader me-mint module per package; key = `(path, package)`. `ast_index` menjadi rentang item di dalam package. | Menjaga `TypeName { module, name }` (`types.rs:10-16`) dan `TypeTable` (`types.rs:118-140`) tetap sah tanpa diubah. |
| 2.8 | Path output = direktori file sumber + nama package. | Menjaga struktur proyek user; keunikan nama sudah dijamin 2.5. |

---

## 3. Visibilitas & penamaan

| # | Keputusan | Alasan |
|---|---|---|
| 3.1 | Visibilitas **per-deklarasi** dengan kata kunci `export`; default private. Berlaku untuk `struct`, `enum`, `fn`, dan binding level-package. | Memetakan 1:1 ke tiga target sekaligus: TS `export`, Go kapitalisasi, Python prefiks `_`. Tanpa ini, emitter Go harus mangle nama semua fungsi snake_case dan tetap butuh aturan anti-tabrakan. |
| 3.2 | Kata kunci `pub` diganti `export` di seluruh permukaan: token/lexer, AST (`is_pub` → `is_export`), `IrClass.is_pub`/`IrMethod.is_pub`, formatter, dan `PLAN.md` #16. | Satu kata kunci saja; tidak ada sinonim `public`. |
| 3.3 | Nama dalam satu package harus unik **tanpa memandang huruf besar/kecil**. | Go memetakan `fn format` → `Format`, yang akan bertabrakan dengan `struct Format`. Tanpa aturan ini output Go bisa tidak compile. |
| 3.4 | Aturan penamaan **benar-benar ditegakkan** (bukan hanya didokumentasikan): tipe `PascalCase`; `fn`/field/binding/parameter/lokal `snake_case`; nama file `snake_case.lt`. | `docs/spec.md:44-52` mengklaim compiler menolaknya, tetapi tidak ada satu pun pemeriksaan penamaan di `crates/*/src/*.rs`. v2 bergantung pada aturan ini (khususnya JSX), jadi harus diimplementasikan. |
| 3.5 | Tidak ada visibilitas per-field dan per-parameter. | Field tidak pernah privat sejak v1; menambahkannya menambah konsep tanpa kebutuhan. |

---

## 4. Tipe & fungsi

| # | Keputusan | Alasan |
|---|---|---|
| 4.1 | `struct` adalah tipe data: daftar field immutable, tanpa method di dalam badan. Generik menempel di struct (`struct Box<T> { items: List<T> }`). | Nominal 1:1 ke `struct`/`record`/dataclass di semua target. |
| 4.2 | Konstruksi tetap literal **named-only**, semua field wajib diisi, tanpa constructor: `Point(x: 1, y: 2)`. | Perilaku v1 (`check.rs:1582-1648`) dipertahankan; menghindari constructor sebagai konsep tambahan. |
| 4.3 | Metode menjadi **fungsi ber-receiver eksplisit**: `fn len(p: Point) -> Int`. Sugar `p.len()` ≡ `len(p)`. | Persis "isi package = fungsi + tipe", tanpa konsep keempat (`impl`), dan memetakan 1:1 ke method Go. Tidak ada `self` implisit, tidak ada dispatch dinamis. |
| 4.4 | Sugar receiver berlaku implisit untuk **semua** fn package-level yang parameter pertamanya bertipe struct/enum. Overloading sah hanya bila tipe receiver berbeda; overloading berdasarkan nama tetap dilarang. | Ruang pencariannya tunggal karena 4.5, sehingga resolusi deterministik. |
| 4.5 | Fungsi ber-receiver **wajib berada di package yang sama** dengan tipe receiver-nya (coherence). | Go mengharuskan method berada di direktori package yang sama dengan tipe. |
| 4.6 | `enum` dipertahankan; varian diakses sebagai `Enum.Variant`. | Mekanisme ADT yang sudah bekerja di 3 target. |
| 4.7 | `Eq`/`Ord`/`Display` di-derive otomatis untuk semua struct/enum. `Eq` dan `Ord` **tidak bisa ditimpa**; `Display` ditimpa lewat `fn to_string(p: Point) -> Str` di package yang sama. | `Ord` dipakai `Map`/`Set`/`sort` dengan tiga jalur implementasi berbeda (`sort.Slice`, `__lt__`, comparator); membiarkannya ditimpa akan memecah paritas. |
| 4.8 | Interpolasi `${expr}` **wajib** menghormati override `Display`. | Bug v1: `docs/spec.md:266-268` menjanjikan override menang, tetapi `${}` selalu memanggil `lt.disp`/`lt.Disp` langsung (`ts.rs:1054-1075`, `py.rs:1022-1043`, `go.rs:1140-1159`). Diperbaiki di v2 dengan satu jalur derivasi. |
| 4.9 | Pseudo-class bawaan menjadi pseudo-package huruf kecil: `math`, `map`, `set`, `json`, `expect` (mis. `map.empty()`, `math.max(a, b)`, `expect.eq(a, b)`). | Konsisten dengan 2.5 dan 3.4; `Math`/`Map`/`Set`/`Json`/`Expect` sebagai identifier PascalCase adalah class, bukan package. **Item terbuka**: bisa dibalik bila nama lama dipertahankan. |

---

## 5. Import & path

| # | Keputusan | Alasan |
|---|---|---|
| 5.1 | Import **hanya package**, bukan file atau simbol lepas. Bentuk utama: `import money { Price, format } from "./wallet/money.lt"`. | Aturan "import hanya bisa package"; destructuring cuma sugar. |
| 5.2 | Path tetap **wajib**. Checker memverifikasi file di path benar-benar memuat `package money`. | Menjaga resolusi deterministik dan kompatibel dengan import URL/lockfile. |
| 5.3 | Bentuk qualifier: `import money from "./wallet/money.lt"` → `money.Price`, `money.format(...)`. | Alternatif destructuring; berguna untuk package besar. |
| 5.4 | Alias diizinkan: `import money as cash from "…"` dan `Price as P` di dalam destructuring. | Dengan nama package unik global, alias adalah satu-satunya obat tabrakan nama import. Mencabut aturan "tanpa alias" v1 (`PLAN.md` §7.4). |
| 5.5 | Shadowing = **error**, bukan "lokal menang" seperti v1 (`check.rs:717-779`). | Konsisten dengan prinsip strict (#3). |
| 5.6 | Path maksimum 3 segmen: `package.{Type\|fn\|binding}` (2), `package.Type.Variant` (3). Tidak ada 4 segmen; `package.Type.field` mustahil karena tak ada static field. | Menutup ambiguitas tanpa membuka sistem path umum. |
| 5.7 | Urutan resolusi segmen pertama: binding lokal → nama hasil destructuring → qualifier package. Ambigu = error. | Membuat `p.len()` vs `pkg.len()` eksplisit. |
| 5.8 | Prefix path di pattern tidak boleh diabaikan diam-diam. | Bug v1: `bind_pattern` memakai `path.last()` (`check.rs:1316-1317`), sehingga `pkg.Type.Variant` di pattern "berhasil" secara salah. |

---

## 6. Statement & ekspresi (v1 dipertahankan)

| # | Keputusan | Alasan |
|---|---|---|
| 6.1 | `let mut`, assignment, `while`, `for`, `break`, `continue` **dipertahankan apa adanya**. | Penghapusan mutabilitas menuntut perubahan di 5 lapisan, `range` di 3 runtime, guard kedalaman rekursi di setiap fungsi yang diemit, penulisan ulang korpus, hilangnya `break`, dan memperlambat output Python — tanpa melayani tujuan menghapus `class`. |
| 6.2 | Blok bernilai hanya di badan `fn`/`if`/`match`; blok lepas tetap statement-only. | Status quo; `let x = { … }` tetap ilegal. |
| 6.3 | `return` dipertahankan untuk guard clause, dan agar `?` tetap bermakna. | Menghapusnya memaksa satu ekspresi raksasa bersarang. |
| 6.4 | `range` bukan kebutuhan bahasa; hanya usulan kosmetik. | Tanpa penghapusan loop, tak ada masalah iterasi numerik yang perlu diselesaikan. |

---

## 7. Test

| # | Keputusan | Alasan |
|---|---|---|
| 7.1 | `test "judul" { ... }` sebagai item **di dalam package**, tanpa parameter. Tidak diemit ke output produksi. | `test fn` tanpa parameter hanyalah pembungkus dan nama fungsi bebas jadi sumber tabrakan; judul langsung dipetakan ke `test(...)`, `def test_…()`, `func Test…`. |
| 7.2 | Judul test unik per package; emitter wajib mendeteksi tabrakan hasil mangling. | v1 membuang nama class di Py (`py.rs:126`) dan Go (`go.rs:181`), dan tak punya cek duplikat — kolisi nama test mungkin terjadi hari ini. |
| 7.3 | `Expect` → `expect.eq`, `expect.not_equal`, `expect.true`, `expect.false`. | Empat nama yang benar-benar diimplementasikan (`check.rs:2125-2157`); pseudo-class menjadi pseudo-package (4.9). |
| 7.4 | `test class` dan `TestClassDecl`/`TestFn` dihapus. | Tidak ada class. |

---

## 8. Template & JSX

| # | Keputusan | Alasan |
|---|---|---|
| 8.1 | Tagged template dipertahankan: tag = **fungsi package-level**, signature `(List<Str>, List<T>) -> R`, setiap `${expr}` wajib persis `T`, invariant `parts.len() == values.len() + 1`. | Mekanisme yang sudah ada dan sudah dipakai (mis. enum `SqlArg`) tidak perlu berubah selain pemilik tag. |
| 8.2 | **JSX = literal struct bertipe (opsi A).** Nama tag = nama `struct` yang terlihat di scope (lokal, hasil destructuring, atau qualifier package). | Menjaga type-safety penuh: tag dan atribut dicek compiler. Opsi "tag = fungsi" ditolak karena bertabrakan dengan model ekspor Go; opsi "satu tipe `Element` untyped" ditolak karena membuang nilai nominal di posisi yang paling sering. |
| 8.3 | Atribut = named argument; nilai berupa string literal atau `{expr}`; semua field wajib terisi. Kata kunci bahasa sebagai nama atribut dilarang. | Konsisten dengan 4.2. |
| 8.4 | Children dikumpulkan ke argumen `children` **hanya jika** struct punya field `children`; jika tidak ada field itu tetapi ada children = compile error. | Aturan v1 (`spec.md:302-317`) dipertahankan. |
| 8.5 | Teks polos = satu child `Str`; hanya sah bila `children` bertipe `List<Str>` (atau `List<T>` dengan `T = Str`). `<X />` = tanpa children. Tag buka/tutup wajib sama. | Aturan v1 dipertahankan. |
| 8.6 | Tag huruf kecil = compile error. | Nama tipe wajib `PascalCase` (3.4) dan Go hanya mengekspor identifier berhuruf besar. `<data>` tidak bisa menunjuk tipe. |

---

## 9. Keputusan yang dibatalkan (dan alasannya)

Dicatat agar tidak diusulkan ulang tanpa konteks baru.

| Dibatalkan | Alasan |
|---|---|
| Hapus `mut`/`while`/`for`/`break` (Q4/Q8 awal) | Biaya di 5 lapisan + menuntut guard rekursi + menghapus pola early-exit + memperlambat output Python; tidak melayani tujuan menghapus `class`. |
| Ban rekursi non-struktural (Q12) | Butuh call graph + SCC + provenance variabel + relasi subterm yang tak ada; tetap tak lengkap untuk rekursi via lambda; versi sintaktis menolak `gcd` dan binary search yang sah. |
| Guard kedalaman runtime (Q12 alternatif) | Tidak diperlukan selama `while` dipertahankan. |
| `for` sebagai sugar `fold` (Q13) | Tidak diperlukan. |
| "Semua isi package public" tanpa kata kunci (Q21 awal) | Emitter Go harus mangle semua nama dan tetap butuh aturan anti-tabrakan; kehilangan satu-satunya cara menyembunyikan helper. Diganti `export` (3.1). |
| `pub` sebagai nama kata kunci | Diganti `export` (3.2) — pilihan rasa, bukan teknis. |
| Tag JSX = fungsi (opsi B) | `export fn card` → `Card` di Go bertabrakan dengan `struct Card`; butuh mangling nama di setiap fungsi publik. |
| Satu tipe `Element` untyped untuk tag huruf kecil (opsi C) | Membuang type-safety tag dan atribut; bertabrakan dengan diferensiasi nominal (1.2). |
| `impl` block ala Rust | Konsep keempat; receiver-di-signature sudah cukup dan memetakan 1:1 ke Go. |

---

## 10. Dampak ke repo

### Permukaan bahasa
- `lt-syntax`: token `package`, `export`, `test`, `struct`; hapus `class`, `pub`, `test class`. AST baru: `Package`, `StructDecl`, `FnDecl`, `Binding`, `TestDecl`. Formatter baru.
- `lt-hir`: package sebagai `ModuleId` (2.7); resolusi path kualifikasi (5.6/5.7); visibilitas `export` lintas package (3.1); penegakan penamaan (3.4); derivasi `Display` yang menghormati override di interpolasi (4.8).
- `lt-ir`: `IrPackage`, `IrStruct`, `IrFn` (free + ber-receiver), hapus `IrClass`.
- `lt-emit-{ts,py,go}`: modul per package; fungsi ber-receiver → method Go / method kelas TS / method dataclass Py; `export` → `export` / kapitalisasi / `_`; derivasi `Eq`/`Ord`/`Display` per struct.
- `lt-cli`: penamaan output dari package, bukan `module.path` (`main.rs:245-277`).

### Korpus yang harus ditulis ulang
- `tests/conformance/*.lt` (13 file), termasuk `loops.lt` (tetap valid, hanya `class` → `package`).
- `playground/cases/**` (43 kasus) dan `playground/diagnostics/**` (9 kasus; `immutable_assign.toml` tetap berlaku).
- `examples/library/*.lt`, `examples/user.lt`, `examples/validator.lt`.
- Golden snapshot di ketiga `crates/lt-emit-*/tests/golden/`.

### Dokumen yang harus ditulis ulang
- `PLAN.md` §2.1 (#9, #15–#17), §2.3, §2.5, §7, §11.
- `docs/spec.md` §4–§13.
- `docs/grammar.ebnf` (seluruh bagian program/items/imports).
- `docs/backends/*.md`, `README.md`.

---

## 11. Item terbuka

1. ~~**Nama pseudo-package**~~ — **diputuskan**: lowercase (`math`, `map`, `set`, `json`, `expect`, `str`, `list`, `option`, `result`).
2. ~~**`Ordering`**~~ — **diputuskan (sementara)**: ditolak sebagai tipe sampai ada konstanta di tiga runtime. Runtime tetap memakainya secara internal. Ada test yang mengunci perilaku ini.
3. ~~**`lt fmt` multi-package**~~ — **diputuskan**: satu baris kosong antar blok `package`, urutan dipertahankan seperti di sumber; ada test formatter.
4. ~~**Diagnostik package ganda / import salah**~~ — **diputuskan**: nama package ganda dilaporkan di deklarasi (dengan path file lain), import yang tidak ada dilaporkan `` package `p` has no `X` ``, import yang tidak di-`export` dilaporkan `` `X` is private to package `p`; add `export` ``.

---

## 12b. Status implementasi

| Lapis | Status |
|---|---|
| `lt-syntax` (lexer, AST, parser, formatter) | ✅ 75 test |
| `lt-hir` (package = module, checker v2) | ✅ 36 test |
| `lt-ir` (struct/fn/binding/test) | ✅ |
| `lt-emit-ts` / `lt-emit-py` / `lt-emit-go` | ✅ (emit + golden) |
| `lt-cli` (naming per package) | ✅ 19 test |
| Korpus: conformance, examples, playground, golden | ✅ dimigrasikan |
| Privasi receiver sugar | ✅ `p.f()` lintas package menolak fungsi non-`export` (checker + lowering) |
| Verifikasi runtime | Go `go test` 13/13 paket; Python `pytest` 45 test; TypeScript `vitest` 45 test + `tsc --noEmit` bersih; playground 43 kasus + 9 diagnostik. |

## 12. Urutan kerja

1. Dokumen: `PLAN.md`, `docs/spec.md`, `docs/grammar.ebnf` disesuaikan dengan ledger ini.
2. `lt-syntax`: lexer + AST + parser + formatter, dengan test parser sendiri.
3. `lt-hir`: package sebagai module, resolusi path, visibilitas, penamaan.
4. `lt-ir` + emitter TypeScript lebih dulu (target paling cepat memberi umpan balik), lalu Python, lalu Go.
5. Korpus: conformance + playground + golden, diperbarui bersamaan dengan tiap emitter.
6. `scripts/check.sh --full` hijau sebelum branch dianggap selesai.
