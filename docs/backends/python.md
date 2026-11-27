# Backend Python

Target kedua. Status: **v2 — terverifikasi**; `mypy --strict` bersih dan 45 test conformance
lulus di `pytest`.

## Representasi

| LT | Python |
|---|---|
| `Int` (i64) | `int` + `_lt.iadd/isub/imul/idiv/irem/ineg` (mask 64-bit) |
| `Float` | `float`; pembagian memakai `_lt.fdiv` agar `x / 0.0` = ±inf/NaN |
| `Str` | `str` (index per code point) |
| `Bool` | `bool` |
| `List<T>` | `_lt.LList` (tuple immutable) |
| `Map<K,V>` | `_lt.LMap` (entry terurut `cmp`) |
| `Set<T>` | `_lt.LSet` |
| `Option<T>` | `_lt.Opt` (`tag` = `"some"`/`"none"`, `values` tuple) |
| `Result<T,E>` | `_lt.Res` (`tag` = `"ok"`/`"err"`) |
| `enum` | `@dataclass(frozen=True)` dengan `tag`/`values`/`order`; varian adalah `Shape.circle(...)` (`@staticmethod`) dan `Shape.Point` (`ClassVar`); display lewat `to_display` |
| `struct` | `@dataclass(frozen=True)`; field menjadi parameter konstruktor bernama, plus `lt_cmp` dan `to_display` |
| fungsi package-level | `def name(...)` (export) / `def _name(...)` (private) |
| binding package | `name = …` / `_name = …` |
| `package` | satu modul: `gen/<package>.py` |
| fungsi ber-receiver | tetap fungsi modul; `p.len()` menjadi `len(p)` saat lowering |
| `Display` | `_lt.disp(value)`; `to_display` memanggil `to_string` milik package bila ada |
| `Eq` / `Ord` | `_lt.eq`, `@dataclass` `__eq__`, `_lt.cmp` |
| `?` | `if t.tag == "err"/"none": return t` |
| template | konkatenasi `+ _lt.disp(...)` |
| tagged template | pemanggilan fungsi package-level dengan `LList[str]` + `LList[T]`; type argument enum generik di-erase ke `Any` |
| `match` | rantai `if/elif` pada `value.tag` |
| JSX | pemanggilan konstruktor dengan keyword argument |
| `expect.eq` dkk | `_lt.expect_equal` / `expect_not_equal` / `expect_true` / `expect_false` |
| `test "judul"` | `def test_judul() -> None` di `tests/test_<package>.py` |

**Shadowing builtin:** package yang mendeklarasikan fungsi bernama `int`/`str`/`list`/… membuat
anotasi primitif di modul itu ditulis sebagai `builtins.int` agar `mypy --strict` tetap valid.

## Struktur paket

```
out/py/
  lt_runtime.py
  pyproject.toml          # pytest: pythonpath=["."], testpaths=["tests"]
  gen/__init__.py
  gen/<package>.py        # satu modul per package LT
  gen/<dir>/__init__.py   # untuk package di subdirektori
  tests/test_<package>.py # hanya dari `lt test`
```

Import antar package: `import gen.models.user as m1`; referensi lintas modul memakai alias itu.
Helper private tetap bisa dipanggil test lewat `m1._helper`.

## Test

```sh
lt test -t py tests/conformance/*.lt --out out/conformance --run
```

`--run` menjalankan `python3 -m pytest -q`. Script `scripts/conformance-py.sh` membuat virtualenv
bila belum ada, menjalankan `mypy --strict .`, lalu `pytest`; dependensi hanya dipasang kalau
belum tersedia.

## Batasan

- Type parameter generik dihapus di Python (duck typing); struct/`enum` generik menjadi kelas biasa.
- Pola `match` bertingkat dan `?` di lambda ditolak di frontend, sama seperti target lain.
- `Int` di Python tak terbatas; masking hanya pada operasi aritmetika yang di-emit.
- `json`/`Value` belum ada; `Ordering` belum tersedia di permukaan.
- Builtin hanya tersedia sebagai receiver (`xs.map(…)`), bukan `list.map(xs, …)`.
