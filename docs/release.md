# Rilis `lt`

Rilis dibangun **lokal**, bukan di CI. Alasannya:

- Build release `lt` di mesin sendiri selesai dalam hitungan detik sampai ~1,5
  menit (build penuh pertama), tanpa antrean runner.
- Tidak memakai menit Actions dan tidak bergantung pada kuota runner.
- Artefak yang diunggah persis yang kamu uji di mesin itu.

## Perintah

```sh
./scripts/release.sh v0.2.0              # build target host + checksum
./scripts/release.sh v0.2.0 --publish    # build + unggah lewat gh
LT_TARGET=x86_64-pc-windows-msvc ./scripts/release.sh v0.2.0
```

Hasil di `dist/`:

```
lt-<target>.tar.gz          (Windows: lt-<target>.zip)
lt-<target>.tar.gz.sha256
```

Nama target mengikuti triple Rust, mis. `x86_64-unknown-linux-gnu`. Skrip
`scripts/install.sh` mencari nama yang sama, jadi tidak ada langkah tambahan.

## Batas cross-compile

| Target | Bisa dibangun di | Catatan |
|---|---|---|
| `x86_64-unknown-linux-gnu` | Linux mana pun | default di mesin pengembang |
| `aarch64-unknown-linux-gnu` | Linux + cross linker | butuh `gcc-aarch64-linux-gnu`; belum terbukti di CI |
| `x86_64-pc-windows-msvc` | Windows, atau Linux dengan `cargo-xwin`/mingw | tidak dicoba otomatis |
| `aarch64-apple-darwin`, `x86_64-apple-darwin` | macOS | SDK Apple tidak boleh didistribusikan, jadi wajib Mac |

Jadi "pindah sepenuhnya ke lokal" berarti **Linux dirilis dari mesinmu, macOS
harus dari Mac**. Kalau tidak ada Mac, `.github/workflows/release.yml` bisa
dijalankan manual (`workflow_dispatch`) untuk target Apple; workflow itu tidak
lagi berjalan otomatis pada tag.

## Kalau butuh aarch64 Linux

Butuh toolchain lintas yang deterministik. Dua opsi yang sudah umum:

- `cargo-zigbuild` + `zig` (`pip install ziglang`), lalu
  `cargo zigbuild --target aarch64-unknown-linux-gnu`, atau
- `cross` (butuh Docker).

Selama itu belum dipasang, rilis aarch64 Linux sengaja dilewati dan
`scripts/install.sh` memberi pesan jelas di arsitektur tersebut.
