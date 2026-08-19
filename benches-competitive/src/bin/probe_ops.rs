//! Price the primitives `copy_node` performs per node, so the 0.52 us/row
//! floor can be attributed without a profiler.
use std::time::Instant;
use taffy::prelude::*;

const N: usize = 3000;
const ITERS: u32 = 200;

fn main() {
    // 1. Nine hashmap ops per node, on the same hasher Lumen uses (FxHash) and
    //    the same key shape (a u32-ish node index).
    type Fx = rustc_hash::FxHashMap<u32, u64>;
    let t0 = Instant::now();
    let mut sink = 0u64;
    for _ in 0..ITERS {
        let mut a: Fx = Fx::default();
        let mut b: Fx = Fx::default();
        for i in 0..N as u32 {
            a.insert(i, i as u64);
            b.insert(i, i as u64);
        }
        for i in 0..N as u32 {
            sink += a.remove(&i).unwrap_or(0);
            sink += b.remove(&i).unwrap_or(0);
        }
    }
    let per_node = t0.elapsed().as_secs_f64() * 1e6 / f64::from(ITERS) / N as f64;
    println!("4 map ops/node (2 insert + 2 remove, FxHash)   {per_node:>7.3} us/node");
    println!("  -> extrapolated to the 9 copy_node performs:  {:>7.3} us/node", per_node * 9.0 / 4.0);

    // 2. Minting a fresh taffy leaf per node, which copy_node does every frame.
    let t1 = Instant::now();
    for _ in 0..ITERS {
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let style = Style { size: Size { width: length(400.0), height: length(16.0) }, ..Default::default() };
        for _ in 0..N {
            let _ = tree.new_leaf(style.clone()).unwrap();
        }
    }
    let taffy_per = t1.elapsed().as_secs_f64() * 1e6 / f64::from(ITERS) / N as f64;
    println!("taffy new_leaf + Style clone                    {taffy_per:>7.3} us/node");

    println!("\nmeasured Lumen floor (empty boxes, no text)     {:>7.3} us/node", 0.516);
    println!("accounted for by the two above                  {:>7.3} us/node", per_node * 9.0 / 4.0 + taffy_per);
    // 3. Just CONSTRUCTING the view's Elements — no runtime, no tree, no
    //    layout. The view closure allocates one Element per row every frame.
    println!("\nsize_of::<Element>()                            {:>7} bytes",
             std::mem::size_of::<lumen_widgets::Element>());
    let t2 = Instant::now();
    for _ in 0..ITERS {
        let v: Vec<lumen_widgets::Element> = (0..N)
            .map(|_| lumen_widgets::Element::column(Vec::new()))
            .collect();
        std::hint::black_box(&v);
    }
    let el_per = t2.elapsed().as_secs_f64() * 1e6 / f64::from(ITERS) / N as f64;
    println!("Element::column construct + drop                {el_per:>7.3} us/node");

    let t3 = Instant::now();
    for _ in 0..ITERS {
        let v: Vec<lumen_widgets::Element> = (0..N)
            .map(|i| lumen_widgets::widgets::text(format!("row {i}")))
            .collect();
        std::hint::black_box(&v);
    }
    let txt_per = t3.elapsed().as_secs_f64() * 1e6 / f64::from(ITERS) / N as f64;
    println!("widgets::text(format!(..)) construct + drop    {txt_per:>7.3} us/node");

    // 4. taffy's actual LAYOUT pass over 3000 leaves in a column — separate
    //    from minting the nodes, and the largest remaining candidate.
    let t4 = Instant::now();
    for _ in 0..ITERS {
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let leaf_style = Style { size: Size { width: length(400.0), height: length(16.0) }, ..Default::default() };
        let kids: Vec<_> = (0..N).map(|_| tree.new_leaf(leaf_style.clone()).unwrap()).collect();
        let root = tree.new_with_children(
            Style { flex_direction: FlexDirection::Column, ..Default::default() }, &kids).unwrap();
        tree.compute_layout(root, Size { width: AvailableSpace::Definite(400.0), height: AvailableSpace::Definite(800.0) }).unwrap();
        std::hint::black_box(tree.layout(kids[0]).unwrap());
    }
    let full_per = t4.elapsed().as_secs_f64() * 1e6 / f64::from(ITERS) / N as f64;
    println!("taffy build tree + compute_layout (3000)      {full_per:>7.3} us/node");
    println!("  of which compute_layout alone               {:>7.3} us/node", full_per - taffy_per);

    println!("\n--- attribution of the 0.516 us/node floor ---");
    println!("  Element construct/drop                       {el_per:>7.3}");
    println!("  taffy new_leaf                               {taffy_per:>7.3}");
    println!("  9 map ops                                    {:>7.3}", per_node * 9.0 / 4.0);
    let compute_only = full_per - taffy_per;
    println!("  taffy compute_layout                         {compute_only:>7.3}");
    let acc = el_per + full_per + per_node * 9.0 / 4.0;
    println!("  accounted                                    {acc:>7.3}");
    println!("  unaccounted                                  {:>7.3}", 0.516 - acc);
    std::hint::black_box(sink);
}
