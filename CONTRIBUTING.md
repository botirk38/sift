# Contributing to Sift

Thank you for your interest in contributing. This document covers the basics for pull requests against [botirk38/sift](https://github.com/botirk38/sift).

## Branch names

Use short, descriptive kebab-case with a type prefix:

| Prefix | Use for |
|--------|---------|
| `feat/` | New behavior, flags, or API |
| `fix/` | Bug fixes, regressions |
| `docs/` | Documentation only |
| `chore/` | Tooling, CI, refactors with no user-visible change |

## Before you push

Run all three checks locally. CI enforces the same on Linux, macOS, and Windows:

```bash
cargo fmt -p sift-core -p sift-grep -- --check
cargo clippy -p sift-core -p sift-grep --all-targets --all-features --no-deps -- -D warnings
cargo test --workspace --all-features \
  --exclude regex --exclude regex-automata --exclude regex-syntax --exclude pcre2
```

## Conventions

- Fix lints at the root cause. `#[allow]` is not permitted.
- Keep changes focused; follow existing patterns in the crate you touch.
- Do not commit `target/`, `.cursor/`, or local `.sift/` directories.
- Prefer domain types and small composable interfaces over parallel `*_with_*` helper variants.

For architecture and module layout guidance, see [`AGENTS.md`](AGENTS.md). CLI-specific notes live in [`crates/cli/AGENTS.md`](crates/cli/AGENTS.md).

## Pull requests

1. Open a PR against `master` from your prefixed branch.
2. Describe the user-visible change and link related issues if any.
3. Add or update tests when behavior changes.
4. Ensure CI passes before requesting review.

## Security

Do not open public issues for security vulnerabilities. See [`SECURITY.md`](SECURITY.md) for reporting instructions.
