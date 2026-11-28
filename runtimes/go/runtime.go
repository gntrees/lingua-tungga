// Package lt is the Lingua Tungga runtime. Generated packages inline it.
package lt

import (
	"encoding/json"
	"fmt"
	"math"
	"sort"
	"strconv"
	"strings"
)

// --- Variant: enums, Option and Result -------------------------------------

type Variant struct {
	Tag   string
	Order int
	// Type is the enum that declared Tag; empty for Option and Result, whose
	// names are fixed. Values is the payload, in declaration order.
	Type   string
	Values []any
}

type Cmper interface {
	LtCmp(any) int
}

// LtDisplay renders the variant structurally: the value carries no renderer,
// only the name of the enum that owns it, so one implementation covers every
// enum.
func (v Variant) LtDisplay() string {
	head := v.Tag
	switch {
	case v.Type != "":
		head = v.Type + "." + v.Tag
	default:
		if name, ok := builtinVariant[v.Tag]; ok {
			head = name
		}
	}
	if len(v.Values) == 0 {
		return head
	}
	parts := make([]string, len(v.Values))
	for index, value := range v.Values {
		parts[index] = Disp(value)
	}
	return head + "(" + strings.Join(parts, ", ") + ")"
}

var builtinVariant = map[string]string{
	"some": "Some",
	"none": "None",
	"ok":   "Ok",
	"err":  "Err",
}

func (v Variant) LtEq(other any) bool {
	o, ok := other.(Variant)
	if !ok || o.Tag != v.Tag || len(o.Values) != len(v.Values) {
		return false
	}
	for index := range v.Values {
		if !Eq(v.Values[index], o.Values[index]) {
			return false
		}
	}
	return true
}

type Eqer interface {
	LtEq(any) bool
}

type Displayer interface {
	LtDisplay() string
}

func Eq(a, b any) bool {
	if left, ok := a.(Eqer); ok {
		return left.LtEq(b)
	}
	if right, ok := b.(Eqer); ok {
		return right.LtEq(a)
	}
	return a == b
}

func Print(value any) {
	fmt.Println(Disp(value))
}

func Disp(value any) string {
	switch v := value.(type) {
	case nil:
		return "None"
	case bool:
		if v {
			return "true"
		}
		return "false"
	case int64:
		return strconv.FormatInt(v, 10)
	case float64:
		if math.IsNaN(v) {
			return "NaN"
		}
		if math.IsInf(v, 1) {
			return "Infinity"
		}
		if math.IsInf(v, -1) {
			return "-Infinity"
		}
		if v == math.Trunc(v) {
			return strconv.FormatFloat(v, 'f', 1, 64)
		}
		return strconv.FormatFloat(v, 'g', -1, 64)
	case string:
		return v
	case Displayer:
		return v.LtDisplay()
	default:
		return fmt.Sprint(value)
	}
}

func Cmp(a, b any) int {
	if left, ok := a.(Cmper); ok {
		return left.LtCmp(b)
	}
	if left, ok := a.(Variant); ok {
		if right, ok := b.(Variant); ok {
			if left.Order != right.Order {
				return compareOrdered(left.Order < right.Order, left.Order > right.Order)
			}
			limit := len(left.Values)
			if len(right.Values) < limit {
				limit = len(right.Values)
			}
			for index := 0; index < limit; index++ {
				if step := Cmp(left.Values[index], right.Values[index]); step != 0 {
					return step
				}
			}
			return len(left.Values) - len(right.Values)
		}
	}
	switch left := a.(type) {
	case int64:
		if right, ok := b.(int64); ok {
			return compareOrdered(left < right, left > right)
		}
	case float64:
		if right, ok := b.(float64); ok {
			return compareOrdered(left < right, left > right)
		}
	case string:
		if right, ok := b.(string); ok {
			return strings.Compare(left, right)
		}
	case bool:
		if right, ok := b.(bool); ok {
			if left == right {
				return 0
			}
			if !left {
				return -1
			}
			return 1
		}
	}
	return strings.Compare(Disp(a), Disp(b))
}

func compareOrdered(less, greater bool) int {
	if less {
		return -1
	}
	if greater {
		return 1
	}
	return 0
}

// FDiv keeps float division through a function call so Go cannot reject
// constant expressions such as 1.0/0.0 at compile time.
func FDiv(a, b float64) float64 {
	return a / b
}

// --- Option ----------------------------------------------------------------

func Some[T any](value T) Variant {
	return Variant{Tag: "some", Order: 1, Values: []any{value}}
}

func None() Variant {
	return Variant{Tag: "none", Order: 0}
}

func OptIsSome(value Variant) bool { return value.Tag == "some" }
func OptIsNone(value Variant) bool { return value.Tag == "none" }

func OptUnwrap[T any](value Variant) T {
	if value.Tag == "none" {
		panic("unwrap on None")
	}
	return value.Values[0].(T)
}

func OptUnwrapOr[T any](value Variant, fallback T) T {
	if value.Tag == "some" {
		return value.Values[0].(T)
	}
	return fallback
}

func OptMap[T any, U any](value Variant, f func(T) U) Variant {
	if value.Tag == "some" {
		return Some(f(value.Values[0].(T)))
	}
	return None()
}

func OptAndThen[T any, U any](value Variant, f func(T) Variant) Variant {
	if value.Tag == "some" {
		return f(value.Values[0].(T))
	}
	return None()
}

// --- Result ----------------------------------------------------------------

func Ok[T any](value T) Variant {
	return Variant{Tag: "ok", Order: 0, Values: []any{value}}
}

func Err[E any](value E) Variant {
	return Variant{Tag: "err", Order: 1, Values: []any{value}}
}

func ResIsOk(value Variant) bool  { return value.Tag == "ok" }
func ResIsErr(value Variant) bool { return value.Tag == "err" }

func ResUnwrap[T any, E any](value Variant) T {
	if value.Tag == "err" {
		panic("unwrap on Err")
	}
	return value.Values[0].(T)
}

func ResUnwrapOr[T any, E any](value Variant, fallback T) T {
	if value.Tag == "ok" {
		return value.Values[0].(T)
	}
	return fallback
}

func ResMap[T any, U any, E any](value Variant, f func(T) U) Variant {
	if value.Tag == "ok" {
		return Ok(f(value.Values[0].(T)))
	}
	return Err(value.Values[0].(E))
}

func ResMapErr[T any, E any, F any](value Variant, f func(E) F) Variant {
	if value.Tag == "ok" {
		return Ok(value.Values[0].(T))
	}
	return Err(f(value.Values[0].(E)))
}

func ResAndThen[T any, U any, E any](value Variant, f func(T) Variant) Variant {
	if value.Tag == "ok" {
		return f(value.Values[0].(T))
	}
	return Err(value.Values[0].(E))
}

// compareItems orders collections lexicographically, so they do not fall back
// to display-string comparison inside Cmp.
func compareItems[T any](left []T, right []T) int {
	limit := len(left)
	if len(right) < limit {
		limit = len(right)
	}
	for index := 0; index < limit; index++ {
		if step := Cmp(left[index], right[index]); step != 0 {
			return step
		}
	}
	return len(left) - len(right)
}

func compareDictEntries[V any](left []DictEntry[V], right []DictEntry[V]) int {
	limit := len(left)
	if len(right) < limit {
		limit = len(right)
	}
	for index := 0; index < limit; index++ {
		if step := strings.Compare(left[index].Key, right[index].Key); step != 0 {
			return step
		}
		if step := Cmp(left[index].Value, right[index].Value); step != 0 {
			return step
		}
	}
	return len(left) - len(right)
}

func compareEntries[K any, V any](left []Entry[K, V], right []Entry[K, V]) int {
	limit := len(left)
	if len(right) < limit {
		limit = len(right)
	}
	for index := 0; index < limit; index++ {
		if step := Cmp(left[index].Key, right[index].Key); step != 0 {
			return step
		}
		if step := Cmp(left[index].Value, right[index].Value); step != 0 {
			return step
		}
	}
	return len(left) - len(right)
}

// --- Immutable collections -------------------------------------------------

type List[T any] struct {
	Items []T
}

func NewList[T any](items []T) List[T] {
	cloned := make([]T, len(items))
	copy(cloned, items)
	return List[T]{Items: cloned}
}

func (l List[T]) Len() int64 { return int64(len(l.Items)) }

func (l List[T]) Get(index int64) Variant {
	if index < 0 || int(index) >= len(l.Items) {
		return None()
	}
	return Some(l.Items[index])
}

func (l List[T]) Push(value T) List[T] {
	return NewList(append(append([]T{}, l.Items...), value))
}

func (l List[T]) Pop() Variant {
	if len(l.Items) == 0 {
		return None()
	}
	return Some(l.Items[len(l.Items)-1])
}

func (l List[T]) Filter(f func(T) bool) List[T] {
	kept := make([]T, 0, len(l.Items))
	for _, item := range l.Items {
		if f(item) {
			kept = append(kept, item)
		}
	}
	return NewList(kept)
}

func (l List[T]) Any(f func(T) bool) bool {
	for _, item := range l.Items {
		if f(item) {
			return true
		}
	}
	return false
}

func (l List[T]) All(f func(T) bool) bool {
	for _, item := range l.Items {
		if !f(item) {
			return false
		}
	}
	return true
}

func (l List[T]) Sort() List[T] {
	sorted := append([]T{}, l.Items...)
	sort.SliceStable(sorted, func(i, j int) bool {
		return Cmp(sorted[i], sorted[j]) < 0
	})
	return NewList(sorted)
}

func (l List[T]) Reverse() List[T] {
	reversed := make([]T, len(l.Items))
	for index, item := range l.Items {
		reversed[len(l.Items)-1-index] = item
	}
	return NewList(reversed)
}

func (l List[T]) Concat(other List[T]) List[T] {
	return NewList(append(append([]T{}, l.Items...), other.Items...))
}

func (l List[T]) Contains(value T) bool {
	for _, item := range l.Items {
		if Eq(item, value) {
			return true
		}
	}
	return false
}

func (l List[T]) IndexOf(value T) Variant {
	for index, item := range l.Items {
		if Eq(item, value) {
			return Some(int64(index))
		}
	}
	return None()
}

func (l List[T]) LtEq(other any) bool {
	o, ok := other.(List[T])
	if !ok || len(o.Items) != len(l.Items) {
		return false
	}
	for index := range l.Items {
		if !Eq(l.Items[index], o.Items[index]) {
			return false
		}
	}
	return true
}

func (l List[T]) LtCmp(other any) int {
	o, ok := other.(List[T])
	if !ok {
		return 0
	}
	return compareItems(l.Items, o.Items)
}

func (l List[T]) LtDisplay() string {
	parts := make([]string, len(l.Items))
	for index, item := range l.Items {
		parts[index] = Disp(item)
	}
	return "List(" + strings.Join(parts, ", ") + ")"
}

func MapList[T any, U any](l List[T], f func(T) U) List[U] {
	mapped := make([]U, len(l.Items))
	for index, item := range l.Items {
		mapped[index] = f(item)
	}
	return NewList(mapped)
}

func FoldList[T any, A any](l List[T], init A, f func(A, T) A) A {
	result := init
	for _, item := range l.Items {
		result = f(result, item)
	}
	return result
}

func JoinList(l List[string], separator string) string {
	return strings.Join(l.Items, separator)
}

type Set[T any] struct {
	Items []T
}

func NewSet[T any]() Set[T] { return Set[T]{} }

func setOf[T any](values []T) Set[T] {
	unique := make([]T, 0, len(values))
	for _, value := range values {
		found := false
		for _, existing := range unique {
			if Eq(existing, value) {
				found = true
				break
			}
		}
		if !found {
			unique = append(unique, value)
		}
	}
	sort.SliceStable(unique, func(i, j int) bool { return Cmp(unique[i], unique[j]) < 0 })
	return Set[T]{Items: unique}
}

func (s Set[T]) Len() int64 { return int64(len(s.Items)) }

func (s Set[T]) Insert(value T) Set[T] { return setOf(append(append([]T{}, s.Items...), value)) }

func (s Set[T]) Remove(value T) Set[T] {
	kept := make([]T, 0, len(s.Items))
	for _, item := range s.Items {
		if !Eq(item, value) {
			kept = append(kept, item)
		}
	}
	return setOf(kept)
}

func (s Set[T]) Contains(value T) bool {
	for _, item := range s.Items {
		if Eq(item, value) {
			return true
		}
	}
	return false
}

func (s Set[T]) Union(other Set[T]) Set[T] {
	return setOf(append(append([]T{}, s.Items...), other.Items...))
}

func (s Set[T]) Intersect(other Set[T]) Set[T] {
	kept := make([]T, 0, len(s.Items))
	for _, item := range s.Items {
		if other.Contains(item) {
			kept = append(kept, item)
		}
	}
	return setOf(kept)
}

func (s Set[T]) Difference(other Set[T]) Set[T] {
	kept := make([]T, 0, len(s.Items))
	for _, item := range s.Items {
		if !other.Contains(item) {
			kept = append(kept, item)
		}
	}
	return setOf(kept)
}

func (s Set[T]) LtEq(other any) bool {
	o, ok := other.(Set[T])
	if !ok || len(o.Items) != len(s.Items) {
		return false
	}
	for index := range s.Items {
		if !Eq(s.Items[index], o.Items[index]) {
			return false
		}
	}
	return true
}

func (s Set[T]) LtCmp(other any) int {
	o, ok := other.(Set[T])
	if !ok {
		return 0
	}
	return compareItems(s.Items, o.Items)
}

func (s Set[T]) LtDisplay() string {
	parts := make([]string, len(s.Items))
	for index, item := range s.Items {
		parts[index] = Disp(item)
	}
	return "Set(" + strings.Join(parts, ", ") + ")"
}

type Entry[K any, V any] struct {
	Key   K
	Value V
}

type Map[K any, V any] struct {
	Entries []Entry[K, V]
}

func NewMap[K any, V any]() Map[K, V] { return Map[K, V]{} }

func mapOf[K any, V any](entries []Entry[K, V]) Map[K, V] {
	sorted := append([]Entry[K, V]{}, entries...)
	sort.SliceStable(sorted, func(i, j int) bool {
		return Cmp(sorted[i].Key, sorted[j].Key) < 0
	})
	// Sorted keys make equal keys adjacent, so one pass replaces the old O(n^2)
	// scan. The later entry wins, which keeps Insert last-wins.
	unique := make([]Entry[K, V], 0, len(sorted))
	for _, entry := range sorted {
		last := len(unique) - 1
		if last >= 0 && Eq(unique[last].Key, entry.Key) {
			unique[last] = entry
		} else {
			unique = append(unique, entry)
		}
	}
	return Map[K, V]{Entries: unique}
}

func (m Map[K, V]) Len() int64 { return int64(len(m.Entries)) }

// slot binary-searches the sorted entries; the caller confirms with Eq so a
// pathological key (NaN) still behaves like the old linear scan.
func (m Map[K, V]) slot(key K) int {
	low, high := 0, len(m.Entries)-1
	for low <= high {
		middle := (low + high) / 2
		step := Cmp(m.Entries[middle].Key, key)
		if step == 0 {
			return middle
		}
		if step < 0 {
			low = middle + 1
		} else {
			high = middle - 1
		}
	}
	return -1
}

func (m Map[K, V]) Get(key K) Variant {
	if slot := m.slot(key); slot >= 0 && Eq(m.Entries[slot].Key, key) {
		return Some(m.Entries[slot].Value)
	}
	for _, entry := range m.Entries {
		if Eq(entry.Key, key) {
			return Some(entry.Value)
		}
	}
	return None()
}

func (m Map[K, V]) Insert(key K, value V) Map[K, V] {
	return mapOf(append(append([]Entry[K, V]{}, m.Entries...), Entry[K, V]{Key: key, Value: value}))
}

func (m Map[K, V]) Remove(key K) Map[K, V] {
	kept := make([]Entry[K, V], 0, len(m.Entries))
	for _, entry := range m.Entries {
		if !Eq(entry.Key, key) {
			kept = append(kept, entry)
		}
	}
	return mapOf(kept)
}

func (m Map[K, V]) Keys() List[K] {
	keys := make([]K, len(m.Entries))
	for index, entry := range m.Entries {
		keys[index] = entry.Key
	}
	return NewList(keys)
}

func (m Map[K, V]) Values() List[V] {
	values := make([]V, len(m.Entries))
	for index, entry := range m.Entries {
		values[index] = entry.Value
	}
	return NewList(values)
}

func (m Map[K, V]) ContainsKey(key K) bool {
	if slot := m.slot(key); slot >= 0 && Eq(m.Entries[slot].Key, key) {
		return true
	}
	for _, entry := range m.Entries {
		if Eq(entry.Key, key) {
			return true
		}
	}
	return false
}

func (m Map[K, V]) LtCmp(other any) int {
	o, ok := other.(Map[K, V])
	if !ok {
		return 0
	}
	return compareEntries(m.Entries, o.Entries)
}

func (m Map[K, V]) LtEq(other any) bool {
	o, ok := other.(Map[K, V])
	if !ok || len(o.Entries) != len(m.Entries) {
		return false
	}
	for index := range m.Entries {
		if !Eq(m.Entries[index].Key, o.Entries[index].Key) ||
			!Eq(m.Entries[index].Value, o.Entries[index].Value) {
			return false
		}
	}
	return true
}

func (m Map[K, V]) LtDisplay() string {
	parts := make([]string, len(m.Entries))
	for index, entry := range m.Entries {
		parts[index] = Disp(entry.Key) + ": " + Disp(entry.Value)
	}
	return "Map(" + strings.Join(parts, ", ") + ")"
}

// Dict is a string-keyed, insertion-ordered map. LtEq ignores order; LtCmp is
// canonical (entries compared by key) so equal dicts always compare equal.
type DictEntry[V any] struct {
	Key   string
	Value V
}

type Dict[V any] struct {
	Entries []DictEntry[V]
	index   map[string]V
}

func NewDict[V any](entries []DictEntry[V]) Dict[V] {
	ordered := make([]DictEntry[V], 0, len(entries))
	positions := make(map[string]int, len(entries))
	for _, entry := range entries {
		if position, ok := positions[entry.Key]; ok {
			// Updating a key keeps its original position, like a Python dict.
			ordered[position] = entry
		} else {
			positions[entry.Key] = len(ordered)
			ordered = append(ordered, entry)
		}
	}
	index := make(map[string]V, len(ordered))
	for _, entry := range ordered {
		index[entry.Key] = entry.Value
	}
	return Dict[V]{Entries: ordered, index: index}
}

func (d Dict[V]) Len() int64 { return int64(len(d.Entries)) }

func (d Dict[V]) Get(key string) Variant {
	if value, ok := d.index[key]; ok {
		return Some(value)
	}
	return None()
}

func (d Dict[V]) Insert(key string, value V) Dict[V] {
	return NewDict(append(append([]DictEntry[V]{}, d.Entries...), DictEntry[V]{Key: key, Value: value}))
}

func (d Dict[V]) Remove(key string) Dict[V] {
	kept := make([]DictEntry[V], 0, len(d.Entries))
	for _, entry := range d.Entries {
		if entry.Key != key {
			kept = append(kept, entry)
		}
	}
	return NewDict(kept)
}

func (d Dict[V]) Keys() List[string] {
	keys := make([]string, len(d.Entries))
	for index, entry := range d.Entries {
		keys[index] = entry.Key
	}
	return NewList(keys)
}

func (d Dict[V]) Values() List[V] {
	values := make([]V, len(d.Entries))
	for index, entry := range d.Entries {
		values[index] = entry.Value
	}
	return NewList(values)
}

func (d Dict[V]) ContainsKey(key string) bool {
	_, ok := d.index[key]
	return ok
}

func (d Dict[V]) LtEq(other any) bool {
	o, ok := other.(Dict[V])
	if !ok || len(o.Entries) != len(d.Entries) {
		return false
	}
	for _, entry := range d.Entries {
		value, found := o.index[entry.Key]
		if !found || !Eq(entry.Value, value) {
			return false
		}
	}
	return true
}

func (d Dict[V]) LtCmp(other any) int {
	o, ok := other.(Dict[V])
	if !ok {
		return 0
	}
	left := append([]DictEntry[V]{}, d.Entries...)
	right := append([]DictEntry[V]{}, o.Entries...)
	sort.SliceStable(left, func(i, j int) bool { return left[i].Key < left[j].Key })
	sort.SliceStable(right, func(i, j int) bool { return right[i].Key < right[j].Key })
	return compareDictEntries(left, right)
}

func (d Dict[V]) LtDisplay() string {
	parts := make([]string, len(d.Entries))
	for index, entry := range d.Entries {
		parts[index] = entry.Key + ": " + Disp(entry.Value)
	}
	return "Dict(" + strings.Join(parts, ", ") + ")"
}

// MapFrom is one bulk build: collect every pair, then sort and dedupe once.
func MapFrom[K any, V any, E any](entries List[E], key func(E) K, value func(E) V) Map[K, V] {
	pairs := make([]Entry[K, V], 0, len(entries.Items))
	for _, entry := range entries.Items {
		pairs = append(pairs, Entry[K, V]{Key: key(entry), Value: value(entry)})
	}
	return mapOf(pairs)
}

// --- Str helpers (indexing is by Unicode code point) -----------------------

func StrLen(value string) int64 { return int64(len([]rune(value))) }

func StrTrim(value string) string { return strings.TrimSpace(value) }

func StrLower(value string) string { return strings.ToLower(value) }

func StrUpper(value string) string { return strings.ToUpper(value) }

func StrStartsWith(value string, prefix string) bool { return strings.HasPrefix(value, prefix) }

func StrEndsWith(value string, suffix string) bool { return strings.HasSuffix(value, suffix) }

func StrContains(value string, needle string) bool { return strings.Contains(value, needle) }

func StrSlice(value string, start int64, end int64) string {
	runes := []rune(value)
	if start < 0 {
		start = 0
	}
	if end > int64(len(runes)) {
		end = int64(len(runes))
	}
	if start > end {
		start = end
	}
	return string(runes[start:end])
}

func StrChars(value string) List[string] {
	runes := []rune(value)
	chars := make([]string, len(runes))
	for index, char := range runes {
		chars[index] = string(char)
	}
	return NewList(chars)
}

func StrTrimIndent(value string) string {
	lines := strings.Split(value, "\n")
	if len(lines) > 0 && strings.TrimSpace(lines[0]) == "" {
		lines = lines[1:]
	}
	for len(lines) > 0 && strings.TrimSpace(lines[len(lines)-1]) == "" {
		lines = lines[:len(lines)-1]
	}
	common := -1
	for _, line := range lines {
		if strings.TrimSpace(line) == "" {
			continue
		}
		width := len(line) - len(strings.TrimLeft(line, " \t"))
		if common < 0 || width < common {
			common = width
		}
	}
	if common <= 0 {
		return strings.Join(lines, "\n")
	}
	for index, line := range lines {
		if len(line) >= common {
			lines[index] = line[common:]
		}
	}
	return strings.Join(lines, "\n")
}

func StrSplit(value string, separator string) List[string] {
	return NewList(strings.Split(value, separator))
}

func StrReplace(value string, old string, new string) string {
	if old == "" {
		return value
	}
	return strings.ReplaceAll(value, old, new)
}

func StrParseInt(value string) Variant {
	text := strings.TrimSpace(value)
	if text == "" {
		return None()
	}
	parsed, err := strconv.ParseInt(text, 10, 64)
	if err != nil {
		return None()
	}
	return Some(parsed)
}

func StrParseFloat(value string) Variant {
	text := strings.TrimSpace(value)
	if text == "" {
		return None()
	}
	parsed, err := strconv.ParseFloat(text, 64)
	if err != nil {
		return None()
	}
	return Some(parsed)
}

// --- JSON -------------------------------------------------------------------

// ValueFactory is the generated Value enum's constructor set. Passing it in keeps
// the runtime independent of that type.
type ValueFactory struct {
	Null   Variant
	Bool   func(bool) Variant
	Int    func(int64) Variant
	Float  func(float64) Variant
	Str    func(string) Variant
	Array  func(List[Variant]) Variant
	Object func(Dict[Variant]) Variant
}

type jsonError string

type jsonReader struct {
	text  []rune
	pos   int
	make  ValueFactory
	depth int
}

func (r *jsonReader) fail(message string) {
	panic(jsonError(fmt.Sprintf("%s at offset %d", message, r.pos)))
}

func (r *jsonReader) space() {
	for r.pos < len(r.text) {
		switch r.text[r.pos] {
		case ' ', '\t', '\n', '\r':
			r.pos++
		default:
			return
		}
	}
}

func (r *jsonReader) take() rune {
	if r.pos >= len(r.text) {
		r.fail("unexpected end of input")
	}
	ch := r.text[r.pos]
	r.pos++
	return ch
}

func (r *jsonReader) expect(want rune) {
	if r.take() != want {
		r.fail("expected `" + string(want) + "`")
	}
}

func (r *jsonReader) has(prefix string) bool {
	runes := []rune(prefix)
	if r.pos+len(runes) > len(r.text) {
		return false
	}
	for index, ch := range runes {
		if r.text[r.pos+index] != ch {
			return false
		}
	}
	return true
}

func (r *jsonReader) value() Variant {
	r.depth++
	if r.depth > 128 {
		r.fail("nesting is too deep")
	}
	defer func() { r.depth-- }()
	r.space()
	if r.pos >= len(r.text) {
		r.fail("unexpected end of input")
	}
	switch r.text[r.pos] {
	case '{':
		return r.object()
	case '[':
		return r.array()
	case '"':
		return r.make.Str(r.str())
	case '-':
		return r.number()
	}
	if r.text[r.pos] >= '0' && r.text[r.pos] <= '9' {
		return r.number()
	}
	if r.has("true") {
		r.pos += 4
		return r.make.Bool(true)
	}
	if r.has("false") {
		r.pos += 5
		return r.make.Bool(false)
	}
	if r.has("null") {
		r.pos += 4
		return r.make.Null
	}
	r.fail("expected a value")
	return Variant{}
}

func (r *jsonReader) object() Variant {
	r.expect('{')
	entries := make([]DictEntry[Variant], 0, 8)
	r.space()
	if r.pos < len(r.text) && r.text[r.pos] == '}' {
		r.pos++
		return r.make.Object(NewDict(entries))
	}
	for {
		r.space()
		if r.pos >= len(r.text) || r.text[r.pos] != '"' {
			r.fail("expected a key")
		}
		key := r.str()
		r.space()
		r.expect(':')
		entries = append(entries, DictEntry[Variant]{Key: key, Value: r.value()})
		r.space()
		switch r.take() {
		case '}':
			return r.make.Object(NewDict(entries))
		case ',':
		default:
			r.fail("expected `,` or `}`")
		}
	}
}

func (r *jsonReader) array() Variant {
	r.expect('[')
	items := make([]Variant, 0, 8)
	r.space()
	if r.pos < len(r.text) && r.text[r.pos] == ']' {
		r.pos++
		return r.make.Array(NewList(items))
	}
	for {
		items = append(items, r.value())
		r.space()
		switch r.take() {
		case ']':
			return r.make.Array(NewList(items))
		case ',':
		default:
			r.fail("expected `,` or `]`")
		}
	}
}

func (r *jsonReader) number() Variant {
	start := r.pos
	if r.text[r.pos] == '-' {
		r.pos++
	}
	for r.pos < len(r.text) && r.text[r.pos] >= '0' && r.text[r.pos] <= '9' {
		r.pos++
	}
	integral := true
	if r.pos < len(r.text) && r.text[r.pos] == '.' {
		integral = false
		r.pos++
		for r.pos < len(r.text) && r.text[r.pos] >= '0' && r.text[r.pos] <= '9' {
			r.pos++
		}
	}
	if r.pos < len(r.text) && (r.text[r.pos] == 'e' || r.text[r.pos] == 'E') {
		integral = false
		r.pos++
		if r.pos < len(r.text) && (r.text[r.pos] == '+' || r.text[r.pos] == '-') {
			r.pos++
		}
		for r.pos < len(r.text) && r.text[r.pos] >= '0' && r.text[r.pos] <= '9' {
			r.pos++
		}
	}
	raw := string(r.text[start:r.pos])
	if !jsonNumberOK(raw) {
		r.pos = start
		r.fail("invalid number")
	}
	if integral {
		if value, err := strconv.ParseInt(raw, 10, 64); err == nil {
			return r.make.Int(value)
		}
	}
	value, err := strconv.ParseFloat(raw, 64)
	if err != nil {
		r.fail("invalid number")
	}
	return r.make.Float(value)
}

func jsonNumberOK(raw string) bool {
	index := 0
	if index < len(raw) && raw[index] == '-' {
		index++
	}
	digits := 0
	for index < len(raw) && raw[index] >= '0' && raw[index] <= '9' {
		index++
		digits++
	}
	if digits == 0 {
		return false
	}
	if digits > 1 && raw[index-digits] == '0' {
		return false
	}
	if index < len(raw) && raw[index] == '.' {
		index++
		digits = 0
		for index < len(raw) && raw[index] >= '0' && raw[index] <= '9' {
			index++
			digits++
		}
		if digits == 0 {
			return false
		}
	}
	if index < len(raw) && (raw[index] == 'e' || raw[index] == 'E') {
		index++
		if index < len(raw) && (raw[index] == '+' || raw[index] == '-') {
			index++
		}
		digits = 0
		for index < len(raw) && raw[index] >= '0' && raw[index] <= '9' {
			index++
			digits++
		}
		if digits == 0 {
			return false
		}
	}
	return index == len(raw)
}

func (r *jsonReader) str() string {
	r.expect('"')
	var out []rune
	for {
		ch := r.take()
		if ch == '"' {
			return string(out)
		}
		if ch != '\\' {
			if ch < ' ' {
				r.fail("control character in a string")
			}
			out = append(out, ch)
			continue
		}
		escape := r.take()
		switch escape {
		case '"':
			out = append(out, '"')
		case '\\':
			out = append(out, '\\')
		case '/':
			out = append(out, '/')
		case 'b':
			out = append(out, '\b')
		case 'f':
			out = append(out, '\f')
		case 'n':
			out = append(out, '\n')
		case 'r':
			out = append(out, '\r')
		case 't':
			out = append(out, '\t')
		case 'u':
			out = append(out, r.hex4())
		default:
			r.fail("invalid escape")
		}
	}
}

func (r *jsonReader) hex4() rune {
	if r.pos+4 > len(r.text) {
		r.fail("invalid escape")
	}
	value := 0
	for _, ch := range r.text[r.pos : r.pos+4] {
		value <<= 4
		switch {
		case ch >= '0' && ch <= '9':
			value += int(ch - '0')
		case ch >= 'a' && ch <= 'f':
			value += int(ch-'a') + 10
		case ch >= 'A' && ch <= 'F':
			value += int(ch-'A') + 10
		default:
			r.fail("invalid escape")
		}
	}
	r.pos += 4
	return rune(value)
}

func JSONParse(text string, make ValueFactory) Variant {
	defer func() {
		// recovered below
	}()
	reader := &jsonReader{text: []rune(text), make: make}
	var value Variant
	var failure string
	func() {
		defer func() {
			if recovered := recover(); recovered != nil {
				if message, ok := recovered.(jsonError); ok {
					failure = string(message)
					return
				}
				panic(recovered)
			}
		}()
		value = reader.value()
		reader.space()
		if reader.pos != len(reader.text) {
			reader.fail("trailing characters")
		}
	}()
	if failure != "" {
		return Err(failure)
	}
	return Ok(value)
}

func JSONStringify(value Variant) string {
	switch value.Tag {
	case "Null":
		return "null"
	case "Bool":
		if value.Values[0].(bool) {
			return "true"
		}
		return "false"
	case "Int":
		return strconv.FormatInt(value.Values[0].(int64), 10)
	case "Float":
		number := value.Values[0].(float64)
		if math.IsNaN(number) || math.IsInf(number, 0) {
			return "null"
		}
		if number == math.Trunc(number) {
			return strconv.FormatFloat(number, 'f', 1, 64)
		}
		return strconv.FormatFloat(number, 'g', -1, 64)
	case "Str":
		encoded, err := json.Marshal(value.Values[0].(string))
		if err != nil {
			return "\"\""
		}
		return string(encoded)
	case "Array":
		items := value.Values[0].(List[Variant]).Items
		parts := make([]string, len(items))
		for index, item := range items {
			parts[index] = JSONStringify(item)
		}
		return "[" + strings.Join(parts, ",") + "]"
	case "Object":
		entries := value.Values[0].(Dict[Variant]).Entries
		parts := make([]string, len(entries))
		for index, entry := range entries {
			encoded, _ := json.Marshal(entry.Key)
			parts[index] = string(encoded) + ":" + JSONStringify(entry.Value)
		}
		return "{" + strings.Join(parts, ",") + "}"
	}
	panic("json.stringify needs a Value, found " + value.Tag)
}

// StrChar returns one code point; out of range is None.
func StrChar(value string, index int64) Variant {
	chars := []rune(value)
	if index < 0 || index >= int64(len(chars)) {
		return None()
	}
	return Some(string(chars[index]))
}

// --- Test assertions -------------------------------------------------------

func ExpectEqual(actual any, expected any) {
	if !Eq(actual, expected) {
		panic("assertion failed: expected " + Disp(expected) + ", got " + Disp(actual))
	}
}

func ExpectNotEqual(actual any, expected any) {
	if Eq(actual, expected) {
		panic("assertion failed: expected " + Disp(actual) + " to differ from " + Disp(expected))
	}
}

func ExpectTrue(value bool) {
	if !value {
		panic("assertion failed: expected true")
	}
}

func ExpectFalse(value bool) {
	if value {
		panic("assertion failed: expected false")
	}
}
