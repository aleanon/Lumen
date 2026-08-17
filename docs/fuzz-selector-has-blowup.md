# The nightly fuzz job is red on `:has()` nesting, not on a crash

*2026-08-17. Written while adding the pre-push CI gate (PG), because the gate
was asked for in response to these failures and cannot possibly fix them.*

## What is actually failing

`fuzz.yml` has failed 8 of the last 15 nights (2026-08-05, -06, -09, -10, -14,
-15, -16, -17). Every failure is the same target and the same class:

```
artifact_prefix='…/fuzz/artifacts/selector/';
Test unit written to …/fuzz/artifacts/selector/slow-unit-eb2e79ec…
```

and on 2026-08-17 it escalated to a hard stop:

```
SUMMARY: libFuzzer: timeout
Error: Fuzz target exited with exit status: 70
```

**No target has ever crashed.** `lss_parse`, `agent_json` and `decode` are clean
throughout, and the totality contract (E.3: these parsers never panic) is
intact. What libFuzzer is reporting is a *performance* finding: an input that
takes longer than the per-input ceiling. On the nights that produced only
`slow-unit-*`, the run itself completed — the job went red at its final step,
`fail on crash artifacts`, which globs `fuzz/artifacts/*/*` and does not
distinguish `slow-unit-` from `crash-`.

The failing inputs are all the same shape: deeply nested `:has()`, e.g.
`:has(*:has(*:has(…)))`.

## Why: `:has()` is O(N^depth)

`crates/lumen-core/src/semantics.rs`:

- `match_selector` (`:721`) scans **all** nodes and calls `node_matches` on each.
- `node_matches` (`:772`) handles `Part::Has(inner)` by calling
  `match_selector(flat, inner)` — a full rescan of all nodes.
- If `inner` also contains `:has`, that recurses again.

There is no memoisation of `(node, selector)` pairs, no cache of the inner
match set (it is recomputed per candidate node even though it does not depend
on the candidate), and no depth limit in the parser — `parse_paren_selector`
(`:982`) recurses on nesting as far as the input goes.

So `N` nodes at nesting depth `d` costs about `N^d`.

Measured on this box, release build, a flat tree of `N` children under one root,
selector `*:has(*:has(…*))`:

| nodes | d=1 | d=2 | d=3 | d=4 | d=5 | d=6 |
|------:|----:|----:|----:|----:|----:|----:|
| 10 | 0.008 ms | 0.033 ms | 0.33 ms | 2.4 ms | 23 ms | 251 ms |
| 20 | 0.008 ms | 0.116 ms | 2.5 ms | 50 ms | 1.07 s | 24.3 s |
| 40 | 0.015 ms | 0.608 ms | 24.7 ms | 1.15 s | 51.0 s | — |

Each extra level of nesting multiplies by roughly `N`. That is the predicted
`N^d`, not merely "slow".

Note the node counts. Those are *tiny* trees — the fuzz target's own tree is a
column with two children. A real app's semantics tree is hundreds of nodes, so
the depth needed to hang it is 2–3, not 6.

## Why it matters beyond a red badge

`selector.rs`'s own header says it: *"agents send arbitrary selector strings over
the wire."* `resolve_one` is reached from the agent RPC surface (03 §2). A
selector is untrusted input from anything holding the agent endpoint, and a
40-character string can occupy a core for a minute. The parsers were fuzzed for
*totality* (they never panic, and they don't); nobody gated their *cost*.

## What the pre-push gate does and does not do about this

It does **not** catch it, and no pre-push gate can:

- `fuzz.yml` runs on `schedule` + `workflow_dispatch`. It is **not triggered by
  push**. Nothing you do before pushing changes what it does at 03:00 UTC.
- The failing input is newly generated each night from a random seed. It is not
  in the repo, so there is nothing local to replay it against.

`scripts/ci_local.sh --only fuzz` therefore does the one useful local thing:
replays the ~1000 committed corpus inputs through all four targets with
`-runs=0 -timeout=5`, catching a *regression* on a known input in seconds. It is
a regression guard, not a substitute for the nightly search.

## Open — not fixed here

The blowup is real and unfixed. Sketch of the options, cheapest first:

1. **Cap nesting depth in the parser** (reject `:has()` past ~4 deep with a
   selector-parse diagnostic). Contains the DoS immediately, costs almost
   nothing, and no legitimate selector nests that deep. Note that 06 A.3 lists
   "any non-additive change to the selector grammar" as an escalation item — a
   depth cap is a rejection of previously-accepted input, so it needs a call.
2. **Hoist the inner match out of the per-candidate loop.** `match_selector(flat,
   inner)` does not depend on the candidate node, so computing it once per
   `Part::Has` and reusing the set turns `N^d` into roughly `N·d` for this
   shape. This looks like the actual fix and is local to `node_matches`.
3. Memoise `(node, selector)` results for the whole matcher.

Separately, `fuzz.yml`'s last step should distinguish `slow-unit-*` from
`crash-*`, so that a performance finding and a memory-safety finding do not
arrive as the same red.

Until one of those lands, expect `fuzz` to keep going red on most nights. It is
reporting something true.
