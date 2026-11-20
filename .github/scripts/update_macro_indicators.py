#!/usr/bin/env python3
"""Validate FRED's DFII10 CSV and write the bounded app snapshot."""

from __future__ import annotations

import argparse
import csv
import datetime as dt
import io
import math
from pathlib import Path
import time

HEADER = ["observation_date", "DFII10"]
METADATA_PREFIX = "# retrieved_at_seconds="
MAX_ROWS = 3_000


def parse_observations(text: str) -> list[tuple[str, str]]:
    reader = csv.reader(io.StringIO(text.lstrip("\ufeff")))
    if next(reader, None) != HEADER:
        raise ValueError("FRED returned an unexpected DFII10 CSV header")

    rows: list[tuple[str, str]] = []
    for row in reader:
        if not row or all(not cell.strip() for cell in row):
            continue
        if len(row) != 2:
            raise ValueError("FRED returned a malformed DFII10 observation row")
        date_text, value_text = (cell.strip() for cell in row)
        try:
            if dt.date.fromisoformat(date_text).isoformat() != date_text:
                raise ValueError
        except ValueError as error:
            raise ValueError("FRED returned an invalid DFII10 observation date") from error
        if value_text in ("", "."):
            continue
        try:
            value = float(value_text)
        except ValueError as error:
            raise ValueError("FRED returned an invalid DFII10 yield value") from error
        if not math.isfinite(value):
            raise ValueError("FRED returned an invalid DFII10 yield value")
        rows.append((date_text, value_text))

    if not rows:
        raise ValueError("FRED returned no usable DFII10 observations")
    if any(left[0] >= right[0] for left, right in zip(rows, rows[1:])):
        raise ValueError("FRED returned unordered or duplicate DFII10 dates")
    return rows[-MAX_ROWS:]


def read_snapshot(path: Path) -> tuple[int, list[tuple[str, str]]]:
    lines = path.read_text(encoding="utf-8").splitlines()
    if len(lines) < 3 or not lines[0].startswith(METADATA_PREFIX):
        raise ValueError("The DFII10 snapshot is missing retrieval metadata")
    try:
        retrieved_at = int(lines[0][len(METADATA_PREFIX) :])
    except ValueError as error:
        raise ValueError("The DFII10 snapshot has invalid retrieval metadata") from error
    if retrieved_at <= 0:
        raise ValueError("The DFII10 snapshot has invalid retrieval metadata")
    rows = parse_observations("\n".join(lines[1:]) + "\n")
    return retrieved_at, rows


def write_snapshot(source: Path, output: Path) -> None:
    rows = parse_observations(source.read_text(encoding="utf-8"))
    output.parent.mkdir(parents=True, exist_ok=True)
    contents = [f"{METADATA_PREFIX}{int(time.time())}", ",".join(HEADER)]
    contents.extend(f"{date},{value}" for date, value in rows)
    output.write_text("\n".join(contents) + "\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--source", type=Path, help="raw FRED CSV to validate and snapshot")
    mode.add_argument("--validate", type=Path, help="validate an existing app snapshot")
    parser.add_argument("--output", type=Path, help="snapshot path for --source")
    args = parser.parse_args()

    if args.source:
        if not args.output:
            parser.error("--source requires --output")
        write_snapshot(args.source, args.output)
    else:
        if args.output:
            parser.error("--output can only be used with --source")
        read_snapshot(args.validate)


if __name__ == "__main__":
    main()
