# Kache (compiler cache)

Proyek ini memakai [Kache](https://github.com/kunobi-ninja/kache) 2.3.1 sebagai
cache compiler Rust. Semua data besar disimpan di drive ADATA
(`/mnt/A29EE2F79EE2C341`), bukan di partisi home.

## Lokasi

| Hal | Path |
|---|---|
| Binary (`kache`, `kache-engine`, …) | `/mnt/A29EE2F79EE2C341/tools/kache/bin` |
| Store (blob cache) | `/mnt/A29EE2F79EE2C341/tools/kache/store` |
| Config | `/mnt/A29EE2F79EE2C341/tools/kache/config.toml` |
| Log & state daemon | `/mnt/A29EE2F79EE2C341/tools/kache/home/cache` |
| Socket/runtime daemon | `/run/user/1000/kache` (tmpfs, ephemeral) |

Config dipilih oleh `KACHE_CONFIG`; `~/.config/kache/config.toml` adalah symlink
ke file di ADATA, jadi perintah `kache` manual juga menemukannya tanpa env.
Symlink lain: `~/.cache/kache` dan `~/.kache-ipc` → subfolder di ADATA.

Runtime dir **tidak** ditaruh di ADATA: mount NTFS/FUSE melaporkan semua direktori
sebagai `0777`, dan Kache menolak socket directory yang bisa ditulis user lain.
Socket memakai `XDG_RUNTIME_DIR` yang memang ephemeral dan privat.

## Cara kerja

`~/.cargo/config.toml` (satu-satunya file di home) berisi:

```toml
[build]
rustc-wrapper = "/mnt/A29EE2F79EE2C341/tools/kache/bin/kache"

[env]
KACHE_CONFIG = "/mnt/A29EE2F79EE2C341/tools/kache/config.toml"
KACHE_RUNTIME_DIR = "/run/user/1000/kache"
HOST_CC = "/mnt/A29EE2F79EE2C341/tools/kache/bin/kache cc"
HOST_CXX = "/mnt/A29EE2F79EE2C341/tools/kache/bin/kache c++"
CC_KNOWN_WRAPPER_CUSTOM = "kache"
```

`[env]` Cargo diteruskan ke wrapper, jadi `cargo build` biasa otomatis memakai
store di ADATA tanpa perlu meng-export variabel apa pun. Shell rc, service
systemd, dan shim PATH **tidak** disentuh (setup dijalankan dengan
`--no-shell --no-service`).

## Perintah

Binary tidak ada di `PATH` global. Untuk memakai CLI Kache:

```sh
export PATH=/mnt/A29EE2F79EE2C341/tools/kache/bin:$PATH
kache stats                 # hit rate, time saved, ukuran store
kache stats --full          # laporan per crate
kache explain <crate>       # kenapa sebuah unit miss
kache doctor --probe        # cek compile + store + restore
```

`cargo build`, `cargo test`, dan `cargo check` tidak butuh langkah tambahan.

## Hasil terukur (proyek ini)

| Skenario | Waktu |
|---|---|
| Build awal tanpa cache (setelah `cargo clean`) | 8.83 s |
| Build yang sama setelah `cargo clean`, dari cache | 0.99 s |
| Hit rate kumulatif | ~61–75% |

Proyek ini kecil, jadi cache baru terasa saat workspace tumbuh. Angka di atas
berasal dari `kache stats`.

## Catatan

- `kache doctor` memberi satu warning permanen: store berada di filesystem FUSE
  (NTFS), yang dianggap bukan "host-local". Kita mitigasi dengan
  `index_mode = "conservative"` (SQLite `synchronous=NORMAL`) dan `local_only`.
  Kalau `index.db` pernah korup, pindahkan `cache.local_store` ke ext4 lalu
  jalankan `kache gc`.
- Reflink tidak didukung NTFS; Kache memakai hardlink (sudah diverifikasi
  berfungsi di mount ini).
- Cache hanya aktif untuk kompilasi Rust (dan C/C++ bila `HOST_CC` dipakai
  build script).

## Menonaktifkan / membatalkan

Sementara:

```sh
KACHE_DISABLED=1 cargo build
```

Permanen:

```sh
rm ~/.cargo/config.toml            # atau pulihkan dari ~/.cargo/config.toml.pre-kache
rm ~/.config/kache/config.toml ~/.cache/kache ~/.kache-ipc
rm -rf /mnt/A29EE2F79EE2C341/tools/kache
```
