# Recovering a partial crates.io release

`publish.sh` uses `publish_crate.py` (Python 3.11+) for each crate in dependency
order. It reads the package name and version from the manifest, checks that exact
version on crates.io, and skips it only when the registry confirms it exists and
is not yanked. Only HTTP 404 means the version is missing; an unavailable registry
must not cause a blind upload.

Cargo HTTP 429 failures are retried up to five total attempts, with delays of
60, 120, 240 and 480 seconds. A longer server retry date is honored (plus a second
of margin); waits beyond an hour stop with an error so an operator can resume
later. Registry reads also have bounded retries for 429, 5xx and network failures.
After a Cargo failure, the exact version is checked again to handle an upload
that succeeded before Cargo failed or a concurrent publisher. Other Cargo errors
fail immediately if the version is still absent. Published versions are immutable;
the check establishes existence, not that an existing package matches local files.

Run the offline regression tests without registry credentials or Cargo:

```sh
python3 -m unittest discover -s scripts -p 'test_publish_crate.py' -v
bash -n scripts/publish.sh
```

## The v1.4.1 failure

Run 41380690387 published webvtt-parser, svgr-macro, fframes-media,
fframes-media-dir-macro and fframes_three_d before crates.io rejected fframes
with HTTP 429. At investigation time, those five versions existed; fframes,
fframes-editor-controller, fframes_skia_renderer, fframes_native_player,
fframes-dylib, cargo-fframes and the npm editor version were absent.

Re-running that historical tag workflow uses the old scripts from the tag. Merging
this fix does not change that behavior (and the existing main workflow publishes a
new nightly). Recovery must be a separately authorized publishing operation using
the original release sources and the corrected helper. Do not move the release tag
or bump versions merely to bypass a partial upload. Verify registry state again
before recovery; do not assume the state above is still current.

This change makes the Rust publishing phase resumable. npm publishing and GitHub
release creation retain their existing behavior and require separate handling if
either already succeeded in a later attempt. No recovery workflow is dispatched
by these tests.
