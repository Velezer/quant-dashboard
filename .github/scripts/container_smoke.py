"""Exercise the production image defaults on first boot and restart under a memory limit."""

import json
import os
from pathlib import Path
import subprocess
import time
import urllib.error
import urllib.request

evidence = Path(os.environ["BUILD_METRICS_DIR"])
suffix = f'{os.environ["GITHUB_RUN_ID"]}-{os.environ["GITHUB_RUN_ATTEMPT"]}'
container = f"quant-startup-smoke-{suffix}"
volume = f"quant-startup-data-{suffix}"
memory_limit = 32 * 1024 * 1024
phases = []
failure = None
container_created = False
volume_created = False

def docker(*args, check=True):
    result = subprocess.run(["docker", *args], text=True, capture_output=True)
    if check and result.returncode:
        raise RuntimeError(f'docker {args[0]} failed: {result.stderr or result.stdout}')
    return result

def inspect():
    return json.loads(docker("inspect", container).stdout)[0]

def collect(label):
    result = docker("inspect", container, check=False)
    if result.returncode:
        (evidence / f"startup-{label}-inspect.json").write_text(
            json.dumps({"inspect_error": result.stderr or result.stdout}, indent=2)
        )
    else:
        info = json.loads(result.stdout)[0]
        details = {
            "state": info["State"],
            "memory_limit_bytes": info["HostConfig"]["Memory"],
            "memory_and_swap_limit_bytes": info["HostConfig"]["MemorySwap"],
        }
        (evidence / f"startup-{label}-inspect.json").write_text(
            json.dumps(details, indent=2)
        )
        # Read cgroup v2 metrics from the runner so sampling adds no
        # extra process to the small application container.
        memory = {}
        try:
            cgroups = Path(f'/proc/{info["State"]["Pid"]}/cgroup').read_text()
            relative = next(line[3:] for line in cgroups.splitlines() if line.startswith("0::"))
            directory = Path("/sys/fs/cgroup") / relative.lstrip("/")
            for name in ("memory.current", "memory.peak", "memory.events"):
                memory[name] = (directory / name).read_text().strip()
        except (OSError, StopIteration) as error:
            memory["unavailable"] = str(error)
        (evidence / f"startup-{label}-memory.json").write_text(
            json.dumps(memory, indent=2)
        )
    logs = docker("logs", "--timestamps", container, check=False)
    (evidence / f"startup-{label}.log").write_text(logs.stdout + logs.stderr)
    stats = docker("stats", "--no-stream", "--format", "{{json .}}", container, check=False)
    (evidence / f"startup-{label}-stats.txt").write_text(stats.stdout + stats.stderr)

def safe_collect(label):
    try:
        collect(label)
    except Exception as error:
        (evidence / f"startup-{label}-capture-error.txt").write_text(str(error))

def healthy():
    state = inspect()["State"]
    if not state["Running"]:
        raise RuntimeError(
            f'container exited: exit={state["ExitCode"]}, '
            f'OOMKilled={state["OOMKilled"]}, error={state["Error"]!r}'
        )
    with urllib.request.urlopen("http://127.0.0.1:18080/health", timeout=1) as response:
        payload = json.load(response)
        if response.status != 200 or payload.get("status") != "ok":
            raise RuntimeError(f"invalid health response: {payload!r}")
    return payload

def check_phase(label):
    deadline = time.monotonic() + 30
    last_error = None
    try:
        while time.monotonic() < deadline:
            try:
                payload = healthy()
                break
            except (urllib.error.URLError, TimeoutError, ConnectionError) as error:
                last_error = error
                time.sleep(0.5)
        else:
            raise RuntimeError(f"health did not become ready within 30s: {last_error}")
        # Keep checking beyond the reported one-second failure window.
        for _ in range(10):
            time.sleep(0.5)
            payload = healthy()
        (evidence / f"startup-{label}-health.json").write_text(
            json.dumps(payload, indent=2)
        )
        phases.append(label)
    finally:
        safe_collect(label)

try:
    docker("volume", "create", volume)
    volume_created = True
    launched = docker(
        "run", "--detach", "--name", container,
        "--memory=32m", "--memory-swap=32m",
        "--publish", "127.0.0.1:18080:8080",
        "--volume", f"{volume}:/data",
        "--env", "QUANT_DISABLE_BACKGROUND_WORK=1",
        "quant-dashboard:startup-smoke",
        check=False,
    )
    container_created = docker("inspect", container, check=False).returncode == 0
    (evidence / "startup-launch.txt").write_text(launched.stdout + launched.stderr)
    if launched.returncode:
        raise RuntimeError(f"docker run failed: {launched.stderr or launched.stdout}")
    (evidence / "startup-container-id.txt").write_text(launched.stdout)
    check_phase("first-boot")
    docker("restart", container)
    check_phase("restart")
except Exception as error:
    failure = str(error)
finally:
    cleanup = []
    if container_created:
        safe_collect("final")
        result = docker("rm", "--force", "--volumes", container, check=False)
        cleanup.append({"resource": container, "exit_code": result.returncode,
                        "output": result.stdout + result.stderr})
    if volume_created:
        result = docker("volume", "rm", volume, check=False)
        cleanup.append({"resource": volume, "exit_code": result.returncode,
                        "output": result.stdout + result.stderr})
    (evidence / "startup-cleanup.json").write_text(json.dumps(cleanup, indent=2))
    (evidence / "startup-result.json").write_text(json.dumps({
        "passed": failure is None,
        "failure": failure,
        "completed_phases": phases,
        "memory_limit_bytes": memory_limit,
        "scope": "Image defaults, isolated storage, background workers disabled; startup and restart only.",
    }, indent=2))
if failure:
    print(f"Startup smoke failed: {failure}")
    logs = evidence / "startup-final.log"
    if logs.exists():
        print(logs.read_text())
    raise SystemExit(1)
print("Image health passed on first boot and restart within the 32 MiB container limit.")
