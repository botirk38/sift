use std::path::PathBuf;

use criterion::{BenchmarkGroup, BenchmarkId, Criterion, measurement::WallTime};
use std::hint::black_box;

use std::str::FromStr;

use sift_grep::Argv;
use sift_grep::filter::{ByteSize, FilterResolution, TypeCatalog};

use crate::support::parse_cli;

fn bench_filter_type_defs_variants(g: &mut BenchmarkGroup<'_, WallTime>) {
    for name in ["with_clear", "with_add"] {
        let argv = match name {
            "with_clear" => crate::support::args(&[
                "sift",
                "--type-clear",
                "rust",
                "--type-clear",
                "py",
                "--type-clear",
                "js",
            ]),
            "with_add" => crate::support::args(&[
                "sift",
                "--type-add",
                "mytype:*.my",
                "--type-add",
                "rust:*.rsx",
            ]),
            _ => unreachable!(),
        };
        g.bench_with_input(
            BenchmarkId::new("TypeCatalog::from_argv", name),
            &argv,
            |b, argv| {
                b.iter(|| {
                    let catalog = TypeCatalog::from_argv(&Argv::new(black_box(argv))).unwrap();
                    black_box(catalog.definitions().len())
                });
            },
        );
    }
}

pub fn bench(c: &mut Criterion) {
    let mut g = c.benchmark_group("filter");

    g.bench_function("ByteSize/from_str", |b| {
        b.iter(|| {
            let _ = black_box(ByteSize::from_str("42"));
            let _ = black_box(ByteSize::from_str("100K"));
            let _ = black_box(ByteSize::from_str("2MB"));
            let _ = black_box(ByteSize::from_str("1G"));
        });
    });

    let argv_default = crate::support::args(&["sift"]);
    g.bench_with_input(
        BenchmarkId::new("TypeCatalog::from_argv", "default"),
        &argv_default,
        |b, argv| {
            b.iter(|| {
                let catalog = TypeCatalog::from_argv(&Argv::new(black_box(argv))).unwrap();
                black_box(catalog.definitions().len())
            });
        },
    );

    bench_filter_type_defs_variants(&mut g);

    let cli_plain = parse_cli(&["pattern"]);
    let argv_plain = crate::support::args(&["sift", "pattern"]);

    g.bench_function("file_config/default", |b| {
        b.iter(|| {
            black_box(cli_plain.filter_config().file_config(
                FilterResolution::resolve(&Argv::new(black_box(&argv_plain))),
                vec![],
                vec![],
                vec![],
            ))
        });
    });

    let cli_glob = parse_cli(&[
        "-g",
        "*.rs",
        "-g",
        "*.toml",
        "-t",
        "rust",
        "--max-depth",
        "10",
        "--max-filesize",
        "1MB",
        "pattern",
    ]);
    let argv_glob = crate::support::args(&[
        "sift",
        "--glob-case-insensitive",
        "-g",
        "*.rs",
        "-g",
        "*.toml",
        "-t",
        "rust",
        "--max-depth",
        "10",
        "--max-filesize",
        "1MB",
        "pattern",
    ]);
    g.bench_function("file_config/with_glob_and_type", |b| {
        b.iter(|| {
            black_box(cli_glob.filter_config().file_config(
                FilterResolution::resolve(&Argv::new(black_box(&argv_glob))),
                vec![],
                vec![PathBuf::from("")],
                vec![],
            ))
        });
    });

    g.finish();
}
