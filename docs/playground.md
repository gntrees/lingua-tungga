# Playground

Galeri statis kecil-kecil: satu kasus = satu aturan bahasa, dengan source,
penjelasan, output ketiga target, dan status test.

```sh
lt playground                 # generate out/playground/index.html
scripts/playground.sh         # verifikasi 3 target + tulis evidence + generate
```

Hasilnya **satu file HTML** yang bekerja lewat `file://` (tanpa server, tanpa
framework JS).

## Layout

```
playground/
  features.toml            # daftar fitur (id, judul, referensi spec)
  cases/<grup>/<id>.lt     # kasus executable (wajib punya minimal satu `test "…"`)
  cases/<grup>/<id>.toml   # metadata: title, feature, why
  diagnostics/<id>.lt      # kasus yang WAJIB gagal compile
  diagnostics/<id>.toml    # + expect = "potongan pesan error"
  evidence.json            # hasil verifikasi (di-commit)
```

Contoh metadata:

```toml
title = "Pembagian Int memotong ke nol"
feature = "ops/division"
why = "Python membulatkan ke bawah; LT memilih truncation agar sama di semua target."
```

## Menambah kasus

1. Tulis file `.lt` (self-contained; tanpa import) dalam satu blok
   `package <nama> { … }` dengan minimal satu `test "judul" { … }`.
2. Tambahkan sidecar `.toml` dengan `title`, `feature`, `why`.
3. Pastikan `feature` ada di `features.toml` (menambah fitur baru = tambah
   entri di sana).
4. `scripts/playground.sh` untuk memverifikasi semua target dan memperbarui
   evidence, lalu `lt playground`.

Gate build akan **gagal** jika:

- ada fitur di `features.toml` tanpa kasus,
- ada kasus yang tidak typecheck / test gagal,
- ada kasus diagnostik yang pesan errornya tidak cocok dengan `expect`,
- `evidence.json` tidak mencakup tepat semua kasus executable (status basi).

## Evidence

`evidence.json` mencatat commit, versi tool, jumlah kasus per target, dan status
per kasus. Ia di-commit supaya halaman bisa menampilkan bukti eksekusi. Bila
daftar kasus berubah tanpa regenerasi, `lt playground` menolak (kecuali
`--allow-stale`, yang menampilkan banner peringatan).

## "Lengkap" itu terukur

`features.toml` adalah daftar fitur yang diklaim; setiap fitur wajib punya
minimal satu kasus, dan setiap kasus wajib menunjuk fitur yang dikenal. Jadi
kelengkapan bukan klaim, melainkan hasil gate.

## Batasan

- Kasus harus satu file tanpa import; skenario import/URL/lockfile tetap
  didokumentasikan di [`package-manager.md`](package-manager.md) dan diuji oleh
  conformance suite.
- Halaman tidak mengeksekusi kode di browser; status berasal dari eksekusi nyata
  saat `scripts/playground.sh` dijalankan.
