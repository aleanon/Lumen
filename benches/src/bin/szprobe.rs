fn main() {
    println!(
        "Style        = {} B",
        std::mem::size_of::<lumen_style::Style>()
    );
    println!(
        "LayoutStyle  = {} B",
        std::mem::size_of::<lumen_layout::LayoutStyle>()
    );
}
