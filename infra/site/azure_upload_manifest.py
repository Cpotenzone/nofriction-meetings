#!/usr/bin/env python3
"""Upload a captured manifest to Azure Blob Storage with exact HTTP headers."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import subprocess
from pathlib import Path


HEADER_ARGS = {
    "ContentType": "--content-type",
    "CacheControl": "--content-cache-control",
    "ContentEncoding": "--content-encoding",
    "ContentLanguage": "--content-language",
    "ContentDisposition": "--content-disposition",
}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--account", required=True)
    parser.add_argument("--workers", type=int, default=8)
    args = parser.parse_args()
    manifest = json.loads(Path(args.manifest).read_text())
    root = Path(args.source).resolve()

    def upload(entry: dict) -> str:
        source = root / entry["key"]
        command = [
            "az",
            "storage",
            "blob",
            "upload",
            "--auth-mode",
            "login",
            "--account-name",
            args.account,
            "--container-name",
            "$web",
            "--name",
            entry["key"],
            "--file",
            str(source),
            "--overwrite",
            "true",
            "--only-show-errors",
            "--output",
            "none",
        ]
        for field, flag in HEADER_ARGS.items():
            if entry.get(field) is not None:
                command.extend([flag, entry[field]])
        subprocess.run(command, check=True)
        return entry["key"]

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        uploaded = list(executor.map(upload, manifest["objects"]))
    if len(uploaded) != manifest["object_count"]:
        raise RuntimeError("upload count mismatch")
    print(json.dumps({"uploaded": len(uploaded), "account": args.account}))


if __name__ == "__main__":
    main()
