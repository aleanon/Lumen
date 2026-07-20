//! E.3 fuzz-lite: the `.lss` parser never panics on arbitrary input — this
//! bounded property runs in EVERY gate; the libFuzzer target (`fuzz/`) goes
//! deeper on the nightly schedule.
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn lss_parse_never_panics(src in ".{0,400}") {
        let _ = lumen_style::parse("fuzz.lss", &src);
    }

    #[test]
    fn lss_parse_survives_structured_noise(
        sel in "[.#a-z*>: ]{0,24}",
        prop in "[a-z-]{0,16}",
        val in ".{0,32}",
    ) {
        let src = format!("{sel} {{ {prop}: {val}; }}");
        let _ = lumen_style::parse("fuzz.lss", &src);
    }
}
