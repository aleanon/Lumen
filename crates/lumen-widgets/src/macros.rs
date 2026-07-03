//! Composition macros for mixing typed widgets and raw `Element`s in one list,
//! each lowered via `Into<Element>`.

/// A column of heterogeneous children — typed widgets and/or `Element`s:
/// `col![Button::new("ok").primary(), Label::new("hi")]`.
#[macro_export]
macro_rules! col {
    ($($child:expr),* $(,)?) => {
        $crate::widgets::column(::std::vec![$( $crate::Element::from($child) ),*])
    };
}

/// Row counterpart of [`col!`].
#[macro_export]
macro_rules! row {
    ($($child:expr),* $(,)?) => {
        $crate::widgets::row(::std::vec![$( $crate::Element::from($child) ),*])
    };
}

/// Reactive-binding sugar (F5.2): `bind!(rt => expr)` builds a
/// [`Dynamic`](lumen_core::Dynamic) from a closure over the store, for any
/// `.bind_*` prop. Names the runtime param explicitly, so signal reads read as
/// `sig.get(rt)`:
///
/// ```ignore
/// node.bind_background(bind!(rt => if on.get(rt) { GREEN } else { RED }))
/// node.bind_class(bind!(rt => if active.get(rt) { vec!["on".into()] } else { vec![] }))
/// ```
#[macro_export]
macro_rules! bind {
    ($rt:ident => $body:expr) => {
        ::lumen_core::Dynamic::new(move |$rt: &::lumen_core::state::Runtime| $body)
    };
}
