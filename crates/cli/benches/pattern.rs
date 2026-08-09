use criterion::{BatchSize, Criterion};
use std::hint::black_box;

use sift_core::{Searcher, ZeroCounts};
use sift_grep::Argv;
use sift_grep::format::InvertMatch;
use sift_grep::pattern::{PatternArgv, ResolvedPatterns};

use crate::support::{args, parse_cli};

pub fn bench(c: &mut Criterion) {
    let mut g = c.benchmark_group("pattern");

    let argv_none = args(&["sift", "pattern"]);
    let argv_single_i = args(&["sift", "-i", "pattern"]);
    let argv_last_wins = args(&["sift", "-i", "-s", "-S", "-i", "pattern"]);

    g.bench_function("case_mode/default", |b| {
        b.iter(|| {
            black_box(
                PatternArgv::resolve(&Argv::new(black_box(&argv_none)), ZeroCounts::Omit).case_mode,
            )
        });
    });
    g.bench_function("case_mode/single_i", |b| {
        b.iter(|| {
            black_box(
                PatternArgv::resolve(&Argv::new(black_box(&argv_single_i)), ZeroCounts::Omit)
                    .case_mode,
            )
        });
    });
    g.bench_function("case_mode/last_wins", |b| {
        b.iter(|| {
            black_box(
                PatternArgv::resolve(&Argv::new(black_box(&argv_last_wins)), ZeroCounts::Omit)
                    .case_mode,
            )
        });
    });

    let argv_v = args(&["sift", "-v", "pattern"]);
    g.bench_function("invert_match/short_v", |b| {
        b.iter(|| {
            black_box(
                PatternArgv::resolve(&Argv::new(black_box(&argv_v)), ZeroCounts::Omit).invert_match,
            )
        });
    });

    let argv_output_lw = args(&["sift", "-c", "-l", "-o", "-q", "pattern"]);
    g.bench_function("search_mode/last_wins", |b| {
        b.iter(|| {
            black_box(PatternArgv::output_mode(
                &Argv::new(black_box(&argv_output_lw)),
                InvertMatch::Off,
                ZeroCounts::Omit,
            ))
        });
    });

    g.bench_function("patterns/positional", |b| {
        let cli = parse_cli(&["beta"]);
        b.iter(|| black_box(ResolvedPatterns::resolve(black_box(&cli.pattern_config())).unwrap()));
    });

    g.bench_function("patterns/repeated_e", |b| {
        let cli = parse_cli(&["-e", "foo", "-e", "bar", "-e", "baz"]);
        b.iter(|| black_box(ResolvedPatterns::resolve(black_box(&cli.pattern_config())).unwrap()));
    });

    g.bench_function("patterns/from_file", |b| {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "foo\nbar\nbaz\n# comment\n\nqux\n").unwrap();
        let path = tmp.path().to_path_buf();
        b.iter_batched(
            || parse_cli(&["-f", path.to_str().unwrap()]),
            |cli| black_box(ResolvedPatterns::resolve(black_box(&cli.pattern_config())).unwrap()),
            BatchSize::SmallInput,
        );
    });

    let cli_default = parse_cli(&["pattern"]);
    let pattern_argv = PatternArgv::resolve(&Argv::new(&argv_none), ZeroCounts::Omit);
    g.bench_function("Searcher/default", |b| {
        b.iter(|| {
            black_box(
                cli_default
                    .pattern_config()
                    .query(vec!["pattern".to_string()], black_box(&pattern_argv))
                    .and_then(Searcher::new)
                    .unwrap(),
            )
        });
    });

    g.finish();
}
