# Repro: `unable to start FSEvent stream` (notify / macOS FSEvents)

Standalone crate (excluded from the sift workspace) that mirrors how
`sift-daemon` constructs `notify::RecommendedWatcher` and recursively watches
a corpus root.

## Observed failure

From [sift#242](https://github.com/botirkhaltaev/sift/issues/242):

```text
sift-daemon: daemon error: unable to start FSEvent stream
```

In `notify` 9.0.0-rc.4 this string is sent when `FSEventStreamStart` returns
false (`src/fsevent.rs` in the fsevent backend). `watch()` then returns that
error to the caller.

Environment where it was seen:

- macOS (Darwin 25 / macOS 26.x), arm64
- `notify = "9.0.0-rc.4"`
- sift-grep 0.7.0 daemon path (`CorpusWatcher`)

It is **intermittent / environment-sensitive**. A plain interactive shell on the
same machine often succeeds; the failure showed up under agent/daemon spawn
workloads. This crate is the minimal call shape, not a guaranteed fail-every-time
harness.

## Run

```bash
# from sift repo root
cargo run --manifest-path repros/notify-fsevent-stream-start/Cargo.toml -- /tmp/some-dir

# rapid create/watch/drop (daemon restart style)
cargo run --manifest-path repros/notify-fsevent-stream-start/Cargo.toml -- /tmp/some-dir --stress 100
```

Exit `2` prints `FAIL at cycle N: unable to start FSEvent stream` (or similar).

## Related

- sift issue: https://github.com/botirkhaltaev/sift/issues/242
- sift poll-fallback PR (consumer workaround): https://github.com/botirkhaltaev/sift/pull/256
- notify source: `FSEventStreamStart` → `Error::generic("unable to start FSEvent stream")`
