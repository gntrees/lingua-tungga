"""Lingua Tungga runtime (Python). Generated packages inline this file."""

from __future__ import annotations

import math
import textwrap
from dataclasses import dataclass
from functools import cmp_to_key
from typing import Any, Callable, Iterable, Sequence

# --- Int: i64 with wrapping overflow ---------------------------------------

_MASK = (1 << 64) - 1
_SIGN = 1 << 63


def _i64(value: int) -> int:
    value &= _MASK
    return value - (1 << 64) if value >= _SIGN else value


def iadd(a: int, b: int) -> int:
    return _i64(a + b)


def isub(a: int, b: int) -> int:
    return _i64(a - b)


def imul(a: int, b: int) -> int:
    return _i64(a * b)


def idiv(a: int, b: int) -> int:
    if b == 0:
        raise ZeroDivisionError("integer division by zero")
    quotient = abs(a) // abs(b)
    if (a < 0) != (b < 0):
        quotient = -quotient
    return _i64(quotient)


def irem(a: int, b: int) -> int:
    return _i64(a - idiv(a, b) * b)


def ineg(a: int) -> int:
    return _i64(-a)


def fdiv(a: float, b: float) -> float:
    if b != 0.0:
        return a / b
    if a != a or a == 0.0:
        return float("nan")
    positive = (a > 0.0) == (math.copysign(1.0, b) > 0.0)
    return float("inf") if positive else float("-inf")


# --- Display, equality, ordering -------------------------------------------


def disp(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        if math.isnan(value):
            return "NaN"
        if math.isinf(value):
            return "Infinity" if value > 0 else "-Infinity"
        if value.is_integer():
            return f"{value:.1f}"
        return repr(value)
    if isinstance(value, str):
        return value
    if value is None:
        return "None"
    display = getattr(value, "to_display", None)
    if callable(display):
        return display()
    return str(value)


def eq(a: Any, b: Any) -> bool:
    if isinstance(a, float) and isinstance(b, float) and math.isnan(a) and math.isnan(b):
        return False
    return a == b


def cmp(a: Any, b: Any) -> int:
    if isinstance(a, bool) and isinstance(b, bool):
        return (a > b) - (a < b)
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        return (a > b) - (a < b)
    if isinstance(a, str) and isinstance(b, str):
        return (a > b) - (a < b)
    left, right = disp(a), disp(b)
    return (left > right) - (left < right)


# --- Option / Result --------------------------------------------------------


@dataclass(frozen=True)
class Opt:
    tag: str  # "some" | "none"
    values: tuple

    def to_display(self) -> str:
        if self.tag == "none":
            return "None"
        return f"Some({disp(self.values[0])})"


def Some(value: Any) -> Opt:
    return Opt("some", (value,))


def None_( ) -> Opt:
    return Opt("none", ())


def opt_is_some(value: Opt) -> bool:
    return value.tag == "some"


def opt_is_none(value: Opt) -> bool:
    return value.tag == "none"


def opt_unwrap(value: Opt) -> Any:
    if value.tag == "none":
        raise ValueError("unwrap on None")
    return value.values[0]


def opt_unwrap_or(value: Opt, fallback: Any) -> Any:
    return value.values[0] if value.tag == "some" else fallback


def opt_map(value: Opt, f: Callable[[Any], Any]) -> Opt:
    return Some(f(value.values[0])) if value.tag == "some" else None_()


def opt_and_then(value: Opt, f: Callable[[Any], Opt]) -> Opt:
    return f(value.values[0]) if value.tag == "some" else None_()


@dataclass(frozen=True)
class Res:
    tag: str  # "ok" | "err"
    values: tuple

    def to_display(self) -> str:
        if self.tag == "ok":
            return f"Ok({disp(self.values[0])})"
        return f"Err({disp(self.values[0])})"


def Ok(value: Any) -> Res:
    return Res("ok", (value,))


def Err(value: Any) -> Res:
    return Res("err", (value,))


def res_is_ok(value: Res) -> bool:
    return value.tag == "ok"


def res_is_err(value: Res) -> bool:
    return value.tag == "err"


def res_unwrap(value: Res) -> Any:
    if value.tag == "err":
        raise ValueError(f"unwrap on Err({disp(value.values[0])})")
    return value.values[0]


def res_unwrap_or(value: Res, fallback: Any) -> Any:
    return value.values[0] if value.tag == "ok" else fallback


def res_map(value: Res, f: Callable[[Any], Any]) -> Res:
    return Ok(f(value.values[0])) if value.tag == "ok" else Err(value.values[0])


def res_map_err(value: Res, f: Callable[[Any], Any]) -> Res:
    return Ok(value.values[0]) if value.tag == "ok" else Err(f(value.values[0]))


def res_and_then(value: Res, f: Callable[[Any], Res]) -> Res:
    return f(value.values[0]) if value.tag == "ok" else Err(value.values[0])


# --- Immutable collections --------------------------------------------------


@dataclass(frozen=True)
class LList:
    items: tuple

    def len(self) -> int:
        return len(self.items)

    def get(self, index: int) -> Opt:
        if index < 0 or index >= len(self.items):
            return None_()
        return Some(self.items[index])

    def push(self, value: Any) -> "LList":
        return LList(self.items + (value,))

    def pop(self) -> Opt:
        if not self.items:
            return None_()
        return Some(self.items[-1])

    def map(self, f: Callable[[Any], Any]) -> "LList":
        return LList(tuple(f(item) for item in self.items))

    def filter(self, f: Callable[[Any], bool]) -> "LList":
        return LList(tuple(item for item in self.items if f(item)))

    def fold(self, init: Any, f: Callable[[Any, Any], Any]) -> Any:
        result = init
        for item in self.items:
            result = f(result, item)
        return result

    def any(self, f: Callable[[Any], bool]) -> bool:
        return any(f(item) for item in self.items)

    def all(self, f: Callable[[Any], bool]) -> bool:
        return all(f(item) for item in self.items)

    def sort(self) -> "LList":
        return LList(tuple(sorted(self.items, key=cmp_to_key(cmp))))

    def reverse(self) -> "LList":
        return LList(tuple(reversed(self.items)))

    def concat(self, other: "LList") -> "LList":
        return LList(self.items + other.items)

    def contains(self, value: Any) -> bool:
        return any(eq(item, value) for item in self.items)

    def index_of(self, value: Any) -> Opt:
        for index, item in enumerate(self.items):
            if eq(item, value):
                return Some(index)
        return None_()

    def join(self, separator: str) -> str:
        return separator.join(disp(item) for item in self.items)

    def to_display(self) -> str:
        return "List(" + ", ".join(disp(item) for item in self.items) + ")"


@dataclass(frozen=True)
class LSet:
    items: tuple

    @staticmethod
    def empty() -> "LSet":
        return LSet(())

    def _from(self, values: Iterable[Any]) -> "LSet":
        unique: list[Any] = []
        for value in values:
            if not any(eq(existing, value) for existing in unique):
                unique.append(value)
        unique.sort(key=cmp_to_key(cmp))
        return LSet(tuple(unique))

    @staticmethod
    def create(values: Iterable[Any]) -> "LSet":
        return LSet.empty()._from(values)

    def len(self) -> int:
        return len(self.items)

    def insert(self, value: Any) -> "LSet":
        return self._from((*self.items, value))

    def remove(self, value: Any) -> "LSet":
        return self._from(item for item in self.items if not eq(item, value))

    def contains(self, value: Any) -> bool:
        return any(eq(item, value) for item in self.items)

    def union(self, other: "LSet") -> "LSet":
        return self._from((*self.items, *other.items))

    def intersect(self, other: "LSet") -> "LSet":
        return self._from(item for item in self.items if other.contains(item))

    def difference(self, other: "LSet") -> "LSet":
        return self._from(item for item in self.items if not other.contains(item))

    def to_display(self) -> str:
        return "Set(" + ", ".join(disp(item) for item in self.items) + ")"


@dataclass(frozen=True)
class LMap:
    entries: tuple

    @staticmethod
    def empty() -> "LMap":
        return LMap(())

    def _from(self, entries: Iterable[tuple]) -> "LMap":
        sorted_entries = sorted(entries, key=cmp_to_key(lambda a, b: cmp(a[0], b[0])))
        unique: list[tuple] = []
        for entry in sorted_entries:
            replaced = False
            for index, existing in enumerate(unique):
                if eq(existing[0], entry[0]):
                    unique[index] = entry
                    replaced = True
                    break
            if not replaced:
                unique.append(entry)
        return LMap(tuple(unique))

    @staticmethod
    def create(entries: Iterable[tuple]) -> "LMap":
        return LMap.empty()._from(entries)

    def len(self) -> int:
        return len(self.entries)

    def get(self, key: Any) -> Opt:
        for entry_key, entry_value in self.entries:
            if eq(entry_key, key):
                return Some(entry_value)
        return None_()

    def insert(self, key: Any, value: Any) -> "LMap":
        return self._from((*self.entries, (key, value)))

    def remove(self, key: Any) -> "LMap":
        return self._from(entry for entry in self.entries if not eq(entry[0], key))

    def keys(self) -> LList:
        return LList(tuple(entry[0] for entry in self.entries))

    def values(self) -> LList:
        return LList(tuple(entry[1] for entry in self.entries))

    def contains_key(self, key: Any) -> bool:
        return any(eq(entry[0], key) for entry in self.entries)

    def to_display(self) -> str:
        return (
            "Map("
            + ", ".join(f"{disp(key)}: {disp(value)}" for key, value in self.entries)
            + ")"
        )


# --- Str helpers (indexing is by Unicode code point) -----------------------


def str_len(value: str) -> int:
    return len(value)


def str_slice(value: str, start: int, end: int) -> str:
    return value[start:end]


def str_chars(value: str) -> LList:
    return LList(tuple(value))


def str_trim_indent(value: str) -> str:
    return textwrap.dedent(value).strip("\n")


def str_split(value: str, separator: str) -> LList:
    return LList(tuple(value.split(separator)))


def str_replace(value: str, old: str, new: str) -> str:
    return value.replace(old, new) if old else value


def str_parse_int(value: str) -> Opt:
    text = value.strip()
    digits = text[1:] if text[:1] in ("+", "-") else text
    if not digits.isdigit():
        return None_()
    parsed = int(text)
    if parsed < -(1 << 63) or parsed > (1 << 63) - 1:
        return None_()
    return Some(parsed)


def str_parse_float(value: str) -> Opt:
    text = value.strip()
    if not text:
        return None_()
    try:
        return Some(float(text))
    except ValueError:
        return None_()


# --- Test assertions --------------------------------------------------------


def expect_equal(actual: Any, expected: Any) -> None:
    if not eq(actual, expected):
        raise AssertionError(
            f"assertion failed: expected {disp(expected)}, got {disp(actual)}"
        )


def expect_not_equal(actual: Any, expected: Any) -> None:
    if eq(actual, expected):
        raise AssertionError(
            f"assertion failed: expected {disp(actual)} to differ from {disp(expected)}"
        )


def expect_true(value: bool) -> None:
    if not value:
        raise AssertionError("assertion failed: expected true")


def expect_false(value: bool) -> None:
    if value:
        raise AssertionError("assertion failed: expected false")
