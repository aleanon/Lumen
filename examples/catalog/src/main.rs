//! Renders the catalog to PNGs: the loading state (spinners) and, after the
//! inline executor settles the visible rows, the loaded state (records).
use lumen_core::geometry::Size;

fn main() {
    let mut a = catalog::main_app().run_headless(Size::new(520.0, 640.0));
    // run_headless already built once (visible rows loading; their inline fetches
    // ran during dispatch and queued results, not yet applied) — so this first
    // frame shows spinners.
    std::fs::write("/tmp/catalog_loading.png", a.screenshot().to_png()).unwrap();
    // A pump drains the queued results → records render.
    let s = a.pump();
    std::fs::write("/tmp/catalog.png", a.screenshot().to_png()).unwrap();
    println!(
        "catalog: {} nodes -> /tmp/catalog_loading.png (spinners), /tmp/catalog.png (loaded)",
        s.node_count
    );
}
