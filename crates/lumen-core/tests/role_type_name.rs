//! OB1: `Role::type_name()` replaced a per-node-per-frame `format!("{:?}", role)`
//! in `build_semantics`. The replacement is only safe if it is *byte-identical*
//! to the `Debug` output it replaced — `SemanticsNode.type_name` is serialized
//! into the agent tree under `"type"`, and agents match on it.
//!
//! These tests are the guard that lets the hand-written match be trusted.

use lumen_core::semantics::Role;

#[test]
fn role_debug_spelling_is_byte_identical() {
    for role in Role::ALL {
        assert_eq!(
            role.type_name(),
            format!("{role:?}"),
            "Role::type_name() diverged from Debug for {role:?}. \
             This string is serialized into the agent tree as \"type\" and is \
             stable API — fix type_name(), do not re-baseline goldens."
        );
    }
}

#[test]
fn role_list_is_exhaustive() {
    // `Role::ALL` is hand-maintained, so a new variant could be added to the
    // enum, handled in both matches, and still be missing here — which would
    // make the byte-identity test above silently stop covering it.
    //
    // There is no reflection over enum variants in Rust, so this compares
    // against the source of truth directly: the count of `///`-documented
    // variants in the enum block.
    let src = include_str!("../src/semantics.rs");
    let enum_body = src
        .split_once("pub enum Role {")
        .expect("Role enum not found — did it move or get renamed?")
        .1
        .split_once("\n}")
        .expect("unterminated Role enum")
        .0;

    let declared = enum_body
        .lines()
        .map(str::trim)
        .filter(|l| l.ends_with(',') && !l.starts_with("///") && !l.starts_with("//"))
        .count();

    assert_eq!(
        declared,
        Role::ALL.len(),
        "Role has {declared} variants but Role::ALL lists {}. \
         Add the new variant to ALL so it is covered by the byte-identity test.",
        Role::ALL.len()
    );
}

#[test]
fn type_name_is_distinct_from_the_wire_role() {
    // The two must never be collapsed into one method: `as_str` is the
    // snake_case wire role, `type_name` the PascalCase type. Both are stable
    // API and agents match on them independently. TextInput is the clearest
    // case where they differ by more than capitalization.
    assert_eq!(Role::TextInput.as_str(), "text_input");
    assert_eq!(Role::TextInput.type_name(), "TextInput");

    let collapsed = Role::ALL
        .iter()
        .filter(|r| r.as_str() == r.type_name())
        .count();
    assert_eq!(
        collapsed, 0,
        "some role's wire string equals its type name — if that is ever true, \
         a future refactor is likely to merge the two methods and break the \
         other surface"
    );
}
