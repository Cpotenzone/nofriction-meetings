#!/usr/bin/env python3
"""Capture and verify an exact static-site object manifest.

The capture path reads bytes from a local directory and HTTP metadata from the
existing private S3 origin. The verify path reads every object from an HTTPS
origin and compares the complete body plus the user-visible content headers.
No credentials or account keys are written to the manifest.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import subprocess
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path


HEADERS = (
    "ContentType",
    "CacheControl",
    "ContentEncoding",
    "ContentLanguage",
    "ContentDisposition",
)


def run_json(command: list[str]) -> dict:
    result = subprocess.run(command, check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def capture(args: argparse.Namespace) -> None:
    root = Path(args.source).resolve()
    listing = run_json(
        [
            "aws",
            "s3api",
            "list-objects-v2",
            "--profile",
            args.profile,
            "--bucket",
            args.bucket,
            "--output",
            "json",
        ]
    )
    objects = listing.get("Contents", [])

    def one(item: dict) -> dict:
        key = item["Key"]
        local = root / key
        if not local.is_file():
            raise RuntimeError(f"missing local object: {key}")
        head = run_json(
            [
                "aws",
                "s3api",
                "head-object",
                "--profile",
                args.profile,
                "--bucket",
                args.bucket,
                "--key",
                key,
                "--output",
                "json",
            ]
        )
        actual_size = local.stat().st_size
        if actual_size != item["Size"] or actual_size != head["ContentLength"]:
            raise RuntimeError(f"size mismatch for {key}")
        entry = {
            "key": key,
            "size": actual_size,
            "sha256": sha256(local),
            "source_etag": head.get("ETag", "").strip('"'),
            "last_modified": head.get("LastModified"),
        }
        for header in HEADERS:
            if head.get(header) is not None:
                entry[header] = head[header]
        return entry

    with concurrent.futures.ThreadPoolExecutor(max_workers=12) as executor:
        entries = sorted(executor.map(one, objects), key=lambda item: item["key"])

    local_keys = sorted(
        str(path.relative_to(root)).replace("\\", "/")
        for path in root.rglob("*")
        if path.is_file()
    )
    source_keys = [entry["key"] for entry in entries]
    if local_keys != source_keys:
        raise RuntimeError("local/source key inventory mismatch")

    canonical = json.dumps(entries, sort_keys=True, separators=(",", ":")).encode()
    manifest = {
        "schema": "nofriction.azure-static-manifest.v1",
        "captured_at": datetime.now(timezone.utc).isoformat(),
        "source": {"kind": "s3", "bucket": args.bucket, "profile_written": False},
        "object_count": len(entries),
        "total_bytes": sum(entry["size"] for entry in entries),
        "objects_sha256": hashlib.sha256(canonical).hexdigest(),
        "objects": entries,
    }
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(manifest, indent=2) + "\n")


def verify(args: argparse.Namespace) -> None:
    manifest = json.loads(Path(args.manifest).read_text())
    base = args.base_url.rstrip("/") + "/"

    def one(entry: dict) -> dict:
        url = urllib.parse.urljoin(base, urllib.parse.quote(entry["key"], safe="/"))
        request = urllib.request.Request(url, headers={"Accept-Encoding": "identity"})
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                body = response.read()
                actual_headers = {
                    "ContentType": response.headers.get("Content-Type"),
                    "CacheControl": response.headers.get("Cache-Control"),
                    "ContentEncoding": response.headers.get("Content-Encoding"),
                    "ContentLanguage": response.headers.get("Content-Language"),
                    "ContentDisposition": response.headers.get("Content-Disposition"),
                }
                status = response.status
        except urllib.error.HTTPError as error:
            body = error.read()
            actual_headers = {}
            status = error.code
        actual = {
            "key": entry["key"],
            "url": url,
            "status": status,
            "size": len(body),
            "sha256": hashlib.sha256(body).hexdigest(),
            "headers": actual_headers,
        }
        expected_headers = {key: entry.get(key) for key in HEADERS}
        actual["passed"] = (
            status == 200
            and actual["size"] == entry["size"]
            and actual["sha256"] == entry["sha256"]
            and actual_headers == expected_headers
        )
        return actual

    with concurrent.futures.ThreadPoolExecutor(max_workers=12) as executor:
        checks = sorted(executor.map(one, manifest["objects"]), key=lambda item: item["key"])
    receipt = {
        "schema": "nofriction.azure-static-verification.v1",
        "verified_at": datetime.now(timezone.utc).isoformat(),
        "base_url": base,
        "object_count": len(checks),
        "passed_count": sum(check["passed"] for check in checks),
        "total_bytes": sum(check["size"] for check in checks if check["status"] == 200),
        "passed": all(check["passed"] for check in checks),
        "checks": checks,
    }
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(receipt, indent=2) + "\n")
    if not receipt["passed"]:
        raise SystemExit(1)


def main() -> None:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    capture_parser = subparsers.add_parser("capture")
    capture_parser.add_argument("--source", required=True)
    capture_parser.add_argument("--bucket", required=True)
    capture_parser.add_argument("--profile", required=True)
    capture_parser.add_argument("--output", required=True)
    capture_parser.set_defaults(func=capture)
    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("--manifest", required=True)
    verify_parser.add_argument("--base-url", required=True)
    verify_parser.add_argument("--output", required=True)
    verify_parser.set_defaults(func=verify)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
