//! What a node costs in each representation.
use lumen_widgets::{Button, Element, Label, ProgressBar};

fn main() {
    use std::mem::size_of;
    println!(
        "Element            {:>5} B  (uniform, every node)",
        size_of::<Element>()
    );
    println!("Label              {:>5} B", size_of::<Label>());
    println!("Button             {:>5} B", size_of::<Button>());
    println!("ProgressBar        {:>5} B", size_of::<ProgressBar>());
    println!(
        "Box<dyn Direct>    {:>5} B  (inline, per child)",
        size_of::<lumen_widgets::direct::Node>()
    );
}
