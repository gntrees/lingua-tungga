# Backend Python

Target kedua. Conformance suite yang sama (17 test) harus lulus di sini dan di
TypeScript; bila keduanya lulus, semantik inti terbukti portabel.

## Representasi

| LT | Python |
|---|---|
| `Int` (i64) | `int` + `lt.iadd/isub/imul/idiv/irem/ineg` (mask 64-bit) |
| `Float` | `float`; pembagian memakai `lt.fdiv` agar `x / 0.0` = ±inf/NaN |
| `Str` | `str` (index per code point) |
| `Bool` | `bool` |
| `List<T>` | `lt.LList` (tuple immutable) |
| `Map<K,V>` | `lt.LMap` (entry terurut `cmp`) |
| `Set<T>` | `lt.LSet` |
| `Option<T>` | `lt.Opt` (`tag` = `"some"`/`"none"`, `values` tuple) |
| `Result<T,E>` | `lt.Res` (`tag` = `"ok"`/`"err"`) |
| `enum` | `@dataclass(frozen=True)` dengan `tag`/`values` + konstruktor `enum_variant(...)` |
| `class` | `@dataclass(frozen=True)`; field jadi parameter konstruktor bernama |
| `Display` | `lt.disp(value)` (memakai `to_display`) |
| `Eq` | `lt.eq`, dataclass `__eq__` |
| `Ord` | `lt.cmp` |
| `?` | `if t.tag == "err"/"none": return t` |
| template | konkatenasi `+ lt.disp(...)` |
| `match` | rantai `if/elif` pada `value.tag` |
| JSX | pemanggilan konstruktor dengan keyword argument |

## Struktur paket

```
out/py/
  lt_runtime.py
  pyproject.toml          # pytest: pythonpath=["."], testpaths=["tests"]
  gen/__init__.py
  gen/<module>.py         # modul LT
  gen/<dir>/__init__.py   # untuk modul bersarang
  tests/test_<module>.py  # hanya dari `lt test`
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
