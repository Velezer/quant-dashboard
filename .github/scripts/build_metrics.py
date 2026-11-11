"""Summarize GNU time reports and container startup evidence for GitHub Actions."""

import json
import os
from pathlib import Path

evidence = Path(os.environ["BUILD_METRICS_DIR"])
evidence.mkdir(parents=True, exist_ok=True)
metrics = {}
rows = ["| Command | Max RSS (MiB) | Elapsed | Artifact bytes |", "| --- | ---: | --- | ---: |"]
for name, artifact in (
    ("wasm", Path("target/wasm32-unknown-unknown/release/quant_pine_runtime.wasm")),
    ("service", Path("target/release/rust_chart_api")),
    ("journal-tests", None),
):
    report = evidence / f"{name}-time.txt"
    data = {"artifact_bytes": artifact.stat().st_size if artifact and artifact.exists() else None}
    if report.exists():
        for line in report.read_text().splitlines():
            if "Maximum resident set size (kbytes):" in line:
                data["maximum_rss_kib"] = int(line.rsplit(":", 1)[1].strip())
            elif "Elapsed (wall clock) time" in line:
                data["elapsed"] = line.split("): ", 1)[1].strip()
            elif "Exit status:" in line:
                data["exit_status"] = int(line.rsplit(":", 1)[1].strip())
    metrics[name] = data
    rss = data.get("maximum_rss_kib")
    rows.append(f'| {name} | {rss / 1024:.2f}' if rss is not None else f'| {name} | unavailable')
    rows[-1] += f' | {data.get("elapsed", "unavailable")} | {data["artifact_bytes"] if data["artifact_bytes"] is not None else "—"} |'
summary = ["## Release resource evidence", "", *rows, "",
           "GNU time reports the command/process RSS maximum, not the total Blitz builder memory peak.", ""]
startup_path = evidence / "startup-result.json"
if startup_path.exists():
    startup = json.loads(startup_path.read_text())
    metrics["startup"] = startup
    summary.extend([f'Container startup/restart: **{"passed" if startup["passed"] else "failed"}**.',
                    "Limit: 32 MiB, with swap disabled. Storage is isolated and retained across the restart."])
    for phase in ("first-boot", "restart", "final"):
        inspected = evidence / f"startup-{phase}-inspect.json"
        if inspected.exists():
            info = json.loads(inspected.read_text())
            state = info.get("state", {})
            summary.append(f'- {phase}: OOMKilled={state.get("OOMKilled", "unknown")}, exit={state.get("ExitCode", "unknown")}.')
        memory_path = evidence / f"startup-{phase}-memory.json"
        if memory_path.exists():
            memory = json.loads(memory_path.read_text())
            peak = memory.get("memory.peak", "")
            if peak.isdigit():
                summary.append(f'  Cgroup peak: {int(peak) / (1024 * 1024):.2f} MiB.')
    if startup.get("failure"):
        summary.extend(["", f'Failure: `{startup["failure"]}`'])
    summary.extend(["", "This checks image defaults on empty storage with background workers disabled. It does not establish steady-state memory under real worker load or existing production data."])
else:
    summary.append("Container startup check did not run; inspect the preceding failed build/test step.")
(evidence / "metrics.json").write_text(json.dumps(metrics, indent=2))
markdown = "\n".join(summary) + "\n"
(evidence / "summary.md").write_text(markdown)
with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as report:
    report.write(markdown)
print(markdown)
