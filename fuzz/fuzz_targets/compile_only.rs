#![no_main]

use libfuzzer_sys::fuzz_target;
use sift_core::search::{Query, SearchFlags, SearchOptions, Searcher};

/// Static branches combined with random bytes to stress alternation and flag shaping.
const STATIC_BRANCHES: &[&str] = &[r"a.c", r"foo|bar", r"^line$", r"\bword\b", "(", r"[", ""];

fn compile_with_flags(patterns: &[String], opts: &SearchOptions) {
    let Ok(query) = Query::new(patterns.to_vec(), opts.clone()) else {
        return;
    };
    let _ = Searcher::new(query);
}

fuzz_target!(|data: &[u8]| {
    let flags = data
        .first()
        .map(|b| SearchFlags::from_bits_truncate(u16::from(*b)))
        .unwrap_or_default();
    let max_results = data.get(1).map(|b| (*b as usize).min(5000));
    let opts = SearchOptions {
        flags,
        max_results,
        ..SearchOptions::default()
    };

    let rest = data.get(2..).unwrap_or_default();
    let dynamic: String = String::from_utf8_lossy(rest).chars().take(256).collect();

    for s in STATIC_BRANCHES {
        let patterns = vec![s.to_string(), dynamic.clone()];
        compile_with_flags(&patterns, &opts);
    }

    let patterns: Vec<String> = STATIC_BRANCHES.iter().map(|s| (*s).to_string()).collect();
    compile_with_flags(&patterns, &opts);

    let single = dynamic.clone();
    compile_with_flags(&[single], &opts);
});
