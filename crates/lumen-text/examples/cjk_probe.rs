//! What the ICU dictionary segmenter actually buys, measured rather than assumed.
//!
//! `parley`'s `complex-scripts` feature switches its line/word segmenters from
//! `new_for_non_complex_scripts` to `new_dictionary`, and the dictionary data
//! (ICU's `cjdict`) is **3.62 MB of the binary** — the single largest item in
//! it, larger than every Lumen crate combined.
//!
//! The workspace manifest justified the feature with "without it parley panics
//! ('no segmentation model for language: ja') on CJK". Running this says
//! otherwise:
//!
//! ```text
//!            dictionary ON        dictionary OFF
//!   ja     160.0 x 62.4 px      160.0 x 62.4 px   + ICU error on stderr
//!   zh     160.0 x 62.4 px      160.0 x 62.4 px   + ICU error on stderr
//!   th     127.6 x 41.6 px      222.8 x 20.8 px   <- does NOT wrap: overflows
//!   latin  151.6 x 62.4 px      151.6 x 62.4 px
//! ```
//!
//! It does not panic, and Japanese and Chinese **wrap identically** — CJK has
//! break opportunities between most characters without any dictionary. What
//! genuinely breaks is Thai (and by the same mechanism Lao, Khmer, Burmese):
//! the 160 px wrap is ignored and the line overflows to 222.8 px.
//!
//! Line breaking is not the whole story: the same data backs `WordSegmenter`,
//! so word-granularity cursor movement and double-click selection in CJK/Thai
//! degrade too. This probe does not measure that.
//!
//! Run: `cargo run -p lumen-text --example cjk_probe --features pan-unicode`
//! (without `pan-unicode` the bundled Latin face has no CJK glyphs, so the
//! widths are tofu widths and only the *wrapping* is meaningful).
use lumen_text::{TextEngine, TextStyle};

fn main() {
    let mut e = TextEngine::new();
    let ts = TextStyle {
        font_size: 16.0,
        ..Default::default()
    };
    const WRAP: f32 = 160.0;
    for (name, s) in [
        (
            "ja",
            "日本語のテキストは、単語の区切りが空白ではありません。",
        ),
        ("zh", "中文文本没有空格分隔的单词，需要词典分词。"),
        ("th", "ภาษาไทยไม่มีการเว้นวรรคระหว่างคำ"),
        ("latin", "The quick brown fox jumps over the lazy dog."),
    ] {
        let b = e.shaped(s, &ts, Some(WRAP), Default::default());
        let over = if b.width() > WRAP + 0.5 {
            "  <- OVERFLOWS the wrap width"
        } else {
            ""
        };
        println!(
            "{name:<6} {:>6.1} x {:>5.1} px{over}",
            b.width(),
            b.height()
        );
    }
    println!("no panic");
}
