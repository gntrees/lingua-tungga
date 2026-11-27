# Backend Python

Target kedua. Conformance suite yang sama (45 test) harus lulus di sini dan di
TypeScript; bila keduanya lulus, semantik inti terbukti portabel.

> **Status:** dokumen ini menggambarkan emitter **v1** (`class`). Desain v2 (`package` +
> `struct` + fungsi ber-receiver) belum diimplementasikan; baris bertanda →v2 adalah target.
> Lihat [`../v2-decisions.md`](../v2-decisions.md).

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
| `enum` | `@dataclass(frozen=True)` dengan `tag`/`values`/`order` + konstruktor `enum_variant(...)` |
| `struct` | `@dataclass(frozen=True)`; field jadi parameter konstruktor bernama |
| fungsi ber-receiver →v2 | method pada dataclass tersebut |
| `package` →v2 | satu modul per package (`gen/money.py`) |
| `Display` | `__lt.disp(value)` (memakai `to_display`) |
| `Eq` | `_lt.eq`, dataclass `__eq__` |
| `Ord` | `_lt.cmp` |
| `?` | `if t.tag == "err"/"none": return t` |
| template | konkatenasi `+ _lt.disp(...)` |
| `to_string()` | `_lt.disp(value)` |
| tagged template | pemanggilan fungsi package-level dengan `LList[str]` + `LList[T]`; type argument enum generik di-erase ke `Any` |
| `match` | rantai `if/elif` pada `value.tag` |
| JSX | pemanggilan konstruktor dengan keyword argument |

## Struktur paket

```
out/py/
  lt_runtime.py
  pyproject.toml          # pytest: pythonpath=["."], testpaths=["tests"]
  gen/__init__.py
  gen/<package>.py        # satu modul per package LT →v2
  gen/<dir>/__init__.py   # untuk package bersarang
  tests/test_<package>.py # hanya dari `lt test`
```

Import antar modul: `import gen.models.user as m1`; referensi lintas modul
memakai alias tersebut. Test memakai `import gen.<module> as m0`.

## Test

```sh
lt test -t py tests/conformance/*.lt --out out/conformance --run
```

`--run` menjalankan `python3 -m pytest -q`. Bila pytest belum tersedia,
`scripts/conformance-py.sh` membuat virtualenv di direktori output dan
meng-install-nya.

## Batasan v1

- Type parameter generik dihapus di Python (duck typing); kelas generik tetap
  menjadi kelas biasa.
- Pola `match` bertingkat dan `?` di lambda tetap ditolak di frontend, sama
  seperti backend TypeScript.
- `Int` di Python adalah objek tak terbatas; masking hanya terjadi pada operasi
  aritmetika yang di-emit, jadi jangan mengharapkan wrapping pada nilai yang
  dibuat manual di luar LT.
