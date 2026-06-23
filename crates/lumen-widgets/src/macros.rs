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
