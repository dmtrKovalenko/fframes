#!/usr/bin/env python3
"""Resume crates.io releases safely. Requires Python 3.11+; no third-party modules."""

import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import tomllib
from email.utils import parsedate_to_datetime
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen


ATTEMPTS = 5
MAX_DELAY = 3600
PUBLISH_TIMEOUT = 600
USER_AGENT = "fframes-release (https://github.com/dmtrKovalenko/fframes)"


def retry_delay(attempt, retry_after=None):
    """Use bounded exponential backoff, respecting the server's minimum delay."""
    delay = 60 * 2**attempt
    if retry_after:
        try:
            server_delay = int(retry_after)
        except ValueError:
            try:
                server_delay = math.ceil(
                    parsedate_to_datetime(retry_after).timestamp() - time.time()
                )
            except (ValueError, TypeError, OverflowError):
                server_delay = 0
        delay = max(delay, server_delay + 1)
    if delay > MAX_DELAY:
        raise RuntimeError("Server requested a wait longer than one hour; resume later")
    return delay


def wait_to_retry(attempt, retry_after=None):
    delay = retry_delay(attempt, retry_after)
    print(f"Retrying in {delay}s", flush=True)
    time.sleep(delay)


def is_published(name, version):
    """Only an exact, non-yanked version is success; only HTTP 404 is absence."""
    url = f"https://crates.io/api/v1/crates/{quote(name, safe='')}/{quote(version, safe='')}"
    request = Request(url, headers={"User-Agent": USER_AGENT})
    for attempt in range(ATTEMPTS):
        retry_after = None
        try:
            with urlopen(request, timeout=30) as response:
                info = json.load(response)["version"]
            if info["crate"] != name or info["num"] != version:
                raise RuntimeError(f"Unexpected registry response for {name}@{version}")
            if info["yanked"]:
                raise RuntimeError(f"{name}@{version} is yanked; refusing to skip it")
            return True
        except HTTPError as error:
            if error.code == 404:
                return False
            if error.code != 429 and not 500 <= error.code < 600:
                raise
            retry_after = error.headers.get("Retry-After")
            if attempt == ATTEMPTS - 1:
                raise
        except (URLError, TimeoutError):
            if attempt == ATTEMPTS - 1:
                raise
        wait_to_retry(attempt, retry_after)
    raise AssertionError("unreachable")


def publish(manifest, version):
    manifest = Path(manifest).resolve()
    with manifest.open("rb") as source:
        package = tomllib.load(source)["package"]
    name = package["name"]
    if package["version"] != version:
        raise RuntimeError(f"{name}: manifest version does not match {version}")

    for attempt in range(ATTEMPTS):
        if is_published(name, version):
            print(f"Skipping {name}@{version}: already published", flush=True)
            return
        print(f"Publishing {name}@{version} (attempt {attempt + 1}/{ATTEMPTS})", flush=True)
        try:
            result = subprocess.run(
                ["cargo", "publish", "--registry", "crates-io", "--allow-dirty", "--no-verify"],
                cwd=manifest.parent,
                env={**os.environ, "CARGO_TERM_COLOR": "never"},
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                timeout=PUBLISH_TIMEOUT,
            )
        except subprocess.TimeoutExpired as error:
            # run() kills and waits for Cargo before raising. Captured timeout
            # output can be bytes even with text=True, or None if Cargo was silent.
            output = error.stdout or ""
            if isinstance(output, bytes):
                output = output.decode("utf-8", errors="replace")
            print(output, end="", flush=True)
            if is_published(name, version):
                print(f"Confirmed {name}@{version} on crates.io after Cargo timed out", flush=True)
                return
            # Do not retry an upload with an uncertain outcome automatically.
            raise RuntimeError(
                f"cargo publish timed out after {PUBLISH_TIMEOUT}s for {name}@{version}; "
                "version not confirmed on crates.io; inspect registry state before resuming"
            ) from error
        print(result.stdout, end="", flush=True)
        if result.returncode == 0:
            return
        # An upload can succeed even if Cargo fails while waiting for the index,
        # or another publisher can win the race after our initial check.
        if is_published(name, version):
            print(f"Confirmed {name}@{version} on crates.io after Cargo failed", flush=True)
            return
        if not re.search(r"\bstatus(?: code)?[: ]+429\b", result.stdout, re.IGNORECASE):
            raise RuntimeError(f"cargo publish failed for {name} (exit {result.returncode})")
        if attempt == ATTEMPTS - 1:
            raise RuntimeError(f"Rate-limit retries exhausted for {name}@{version}")
        # Cargo includes crates.io's RFC 2822 retry date in its error body.
        match = re.search(r"try again after (.+? GMT)", result.stdout, re.IGNORECASE)
        wait_to_retry(attempt, match.group(1) if match else None)


if __name__ == "__main__":
    try:
        if len(sys.argv) != 3:
            raise RuntimeError("Usage: publish_crate.py <Cargo.toml> <version>")
        publish(sys.argv[1], sys.argv[2])
    except (RuntimeError, OSError, ValueError, KeyError) as error:
        sys.exit(str(error))
