# Type-change hot reload: why keyed state beats a state struct

*2026-08-09. Prior art: `hot_ice` (iced hot-reloader, `/mnt/Utvikling-linux/hot_ice`),
read for this comparison.*

The question was whether per-field/keyed reactive state makes hot-reloading
**type changes** easier. It does, and this is a stronger argument for the design
than any performance number in `docs/patch-path-measurement.md` — because it
changes an outcome qualitatively rather than by a percentage.

## How `hot_ice` does it

State is one user struct, `T: Serialize + DeserializeOwned + Default`. On reload
the old cdylib serializes it whole (`serde_json::to_vec`), the new one
deserializes it whole (`serde_json::from_slice::<T>`). That is the right design
for iced, whose `Application` state *is* one struct.

The consequence is all-or-nothing:

```rust
match serde_json::from_slice(data) {
    Ok(state) => state,
    Err(e) => { result = Err(...); T::default() }   // every field lost
}
```

One incompatible field change — `u32` → `String`, a field removed without
`#[serde(default)]` — and the whole application resets.

`hot_ice` also documents a hazard worth importing wholesale: **`TypeId` is not
stable across cdylib reloads.** The same struct compiled into two loads gets
different `TypeId`s, so `downcast_ref` always returns `None` after a reload, and
it uses an unchecked cast with a written soundness contract instead. Anything in
Lumen that ever swaps a cdylib (tier-2 code substitution, `hotpatch.rs`) inherits
that exactly, and HR1's honesty fix is the same family of problem.

## How Lumen does it

State is N independently keyed signals, and the snapshot is a JSON **object with
one entry per key**:

```rust
map.insert(key, to_json(&*slot.value));   // per signal
StateSnapshot(Value::Object(map))
```

Restore adopts per key, and failure is scoped to the key:

```rust
match deser_lenient::<T>(&key, &json) {
    Ok((t, diags)) => { /* adopt, report field-level drops */ }
    Err(d)         => { restore_diags.push(d); Box::new(init()) }  // this key only
}
```

So a type change to one field costs that one signal, which resets to its
initializer with `W0002: could not restore \`k\` (…); using default`. Every other
signal restores untouched.

There is a second level of granularity inside a single value: `deser_lenient`
re-serializes what it just parsed and diffs the keys, so a struct stored in one
signal that *loses* a field reports `dropped state field \`x\` while restoring
\`k\`` rather than silently discarding it.

| | `hot_ice` | Lumen |
|---|---|---|
| unit of state | one struct | N keyed signals |
| snapshot | whole-struct JSON | JSON object, one entry per key |
| one field's type changes | **all state → `T::default()`** | that key → its initializer; rest restore |
| a field disappears | silent if `from_slice` still parses | `W0002` naming the field |
| diagnosis | one deserialize error | per key, and per field within a key |

## What a `#[derive(Reactive)]` would add on top

The keying is already per field in effect — but the *key* is a string chosen at
the call site (`cx.signal("count", …)`), so the compiler cannot help. A derive
would make two things possible that are awkward today:

* **Renames become expressible.** Today renaming a field is indistinguishable
  from deleting one and adding another: `W0002` plus a default. A derive could
  carry `#[reactive(alias = "old_name")]` and migrate the value.
* **Type changes become diagnosable as such.** A per-field shape hash emitted at
  compile time would let the restore say "`count` changed `u32` → `String`"
  instead of surfacing a serde error string.

## The connection back to inline values

This is the same property that made `Vec<Reactive<u32>>`-with-inline-storage a
bad trade (Addendum 2). Per-key restore only works because the **values live in
the store** where the runtime can enumerate and serialize them. Moving them into
user-owned structs would buy ~1% of frame time and cost exactly the granularity
described here — turning Lumen's per-key restore back into `hot_ice`'s
whole-struct one.

Stated as an invariant: **the store's enumerability is what makes partial
restore possible.** That is the same pillar MOD6 was declined to protect.
