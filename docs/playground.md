# Playground

A small static gallery: one case = one language rule, with its source, an
explanation, the generated code for all three targets, and — for every target —
a runnable program showing how to call the result, plus the values it printed.

```sh
lt playground                 # generate out/playground/index.html
scripts/playground.sh         # verify 3 targets, record usage + evidence, generate
```

The output is **one HTML file** that works over `file://` (no server, no JS
framework).

## Layout

```
playground/
  features.toml            # feature list (id, title, spec reference)
  cases/<group>/<id>.lt    # executable case (needs an `export` and a `test "…"`)
  cases/<group>/<id>.toml  # metadata: title, feature, why
  cases/<group>/_name.lt   # import target for the cases beside it (not a case)
  diagnostics/<id>.lt      # case that MUST fail to compile
  diagnostics/<id>.toml    # + expect = "fragment of the error message"
  evidence.json            # verification results (committed)
  usage.json               # recorded stdout of every usage program (committed)
```

Example metadata:

```toml
title = "Int division truncates toward zero"
feature = "ops/division"
why = "Python floors; Lingua Tungga truncates so every target agrees."
```

## Usage programs per target

Each case has three tabs — TypeScript, Python, Go. Every tab shows two things
stacked: the generated module, and directly below it the *usage program* for that
target together with the stdout it produced.

The usage program imports the emitted module and replays the case's tests, but
each `expect.*` is replaced by a print of its first argument. So it shows the
real call site — the actual arguments, the real receiver, the generic
instantiation — and then the values that call produced:

```ts
import * as lt from "../../_lingua_rt";
import * as m1 from "./question";

function main(): void {
  {
    lt.print(m1.double("21"));
  }
}
```

```text
Ok(42)
```

Because the program is derived from the same IR as the tests, it cannot drift
from the library API: renaming a function, changing a parameter, or adding a
variant updates the usage program on the next generation.

## Adding a case

1. Write a `.lt` file with one `package <name> { … }` block, at least one
   `export` declaration (`export fn`, `export let`, `export struct`,
   `export enum`) and at least one `test "title" { … }` block. A case that needs
   an import keeps the target in a sibling `_name.lt`; helper files never become
   cases themselves, so `cases/imports/qualified.lt` pairs with
   `cases/imports/_wallet.lt`.
2. Add a sidecar `.toml` with `title`, `feature`, `why`.
3. Make sure `feature` exists in `features.toml` (a new feature is a new entry
   there).
4. Run `scripts/playground.sh` to verify every target, record stdout and
   evidence, then `lt playground`.

The build gate **fails** when:

- a feature in `features.toml` has no case,
- a case does not type check, or a generated test fails,
- a case exports nothing — an empty module would make the output and usage
  tabs meaningless,
- a case has no `test` block to derive the usage program from,
- a diagnostic case does not produce the error message named by `expect`,
- a usage program fails to compile or exits non-zero,
- `evidence.json` does not cover exactly the current executable cases (stale
  status).

## Evidence and usage output

`evidence.json` records the commit, tool versions, case counts per target, and
the pass/fail status of every case. `usage.json` records, per case and target,
the exit status and the stdout lines of the usage program. Both are committed so
the page can show real proof without executing anything in the browser.

If the case list changes without regenerating, `lt playground` refuses to run
(unless `--allow-stale`, which renders a warning banner).

## "Complete" is measurable

`features.toml` is the list of claimed features; every feature needs a case and
every case must point at a known feature. On top of that, every case must expose
something callable (the export rule) and demonstrate calling it (the usage
rule), so the catalogue is checked rather than asserted.

## Limits

- A case must be a single file without imports; import/URL/lockfile scenarios
  stay documented in [`package-manager.md`](package-manager.md) and are covered
  by the conformance suite.
- The page does not execute code in the browser: status and stdout come from
  real runs performed by `scripts/playground.sh`.
- Usage programs replay the case's tests. They demonstrate the API, they are not
  a second test suite; the generated `test` files remain the source of truth for
  behaviour.
