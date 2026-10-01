#!/usr/bin/env python3
"""Fixed validation profiles shared by hosted Actions and local act execution.

The plan selects packages, not arbitrary shell commands. Preserve complete suites;
dependency builds are expected, but unrelated dependency test suites are not.
"""

import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tomllib

from gate import validate_plan
from plan import BNB, NOSTD, PKI, PROFILES


def packages(names):
    return [argument for name in sorted(names) for argument in ("-p", name)]


def commands(profile, plan, bnb):
    selected = set(plan["packages"])
    roots = set(plan["roots"])
    workspace = ["--workspace"] if plan["full"] else packages(selected)
    if profile == "workspace":
        return [["cargo", "fmt", "--all", "--check"],
                ["cargo", "clippy", *workspace, "--all-targets"],
                ["cargo", "test", *workspace],
                [sys.executable, "-m", "unittest", "discover", "-s", "scripts/ci", "-p", "integration_*.py", "-v"]]
    if profile == "bnb":
        return [["cargo", "clippy", "-p", "bitsandbytes", "-p", "bitsandbytes-macros",
                 "--all-targets", "--all-features", "--", "-D", "warnings"],
                *[["cargo", "test", "-p", "bitsandbytes", "--features", feature]
                  for feature in ("bytes", "mock", "tokio-io")],
                ["cargo", "test", "-p", "bitsandbytes", "--all-features"],
                ["cargo", "doc", "-p", "bitsandbytes", "-p", "bitsandbytes-macros", "--all-features", "--no-deps"],
                ["cargo", "doc", "-p", "bitsandbytes", "--no-default-features", "--no-deps"]]
    if profile == "socks":
        return [["cargo", "clippy", "-p", "socks", "--all-targets", "--all-features", "--", "-D", "warnings"],
                ["cargo", "test", "-p", "socks", "--features", "blocking"],
                ["cargo", "test", "-p", "socks", "--features", "tokio"],
                ["cargo", "test", "-p", "socks", "--all-features"],
                ["cargo", "doc", "-p", "socks", "--all-features", "--no-deps"]]
    if profile == "network":
        result = []
        if "rawsock" in selected:
            result.append(["cargo", "test", "-p", "rawsock", "--all-features"])
        if "dns" in selected:
            result.append(["cargo", "test", "-p", "dns", "--features", "client"])
        injected = selected & {"ip", "icmp", "tcp", "udp", "ethernet", "arp"}
        if injected:
            result.append(["cargo", "test", "--features", "inject", *packages(injected)])
        return result
    if profile == "facades":
        result = [["cargo", "build", "-p", name, "--features", "full"]
                  for name in sorted(selected & {"rsl", "rsl-deps"})]
        if "demos" in selected:
            result.append(["cargo", "build", "-p", "demos", "--examples"])
        return result
    if profile == "msrv":
        result = [["cargo", "check", *workspace]] if selected else []
        result.extend(["cargo", "check", "-p", name, "--all-features"]
                      for name in sorted(selected & {"bitsandbytes", "socks"}))
        if NOSTD in roots:
            result.append(["cargo", "check", "--manifest-path", f"{NOSTD}/Cargo.toml"])
        return result
    if profile == "no-std":
        result = []
        if BNB in roots:
            result.append(["cargo", "build", "-p", "bitsandbytes", "--no-default-features"])
        if NOSTD in roots:
            result.append(["cargo", "build", "--manifest-path", f"{NOSTD}/Cargo.toml",
                           "--target", "thumbv7em-none-eabi"])
        pki = selected & {"rsl-asn1", "rsl-x509", "rsl-pki"}
        if roots & PKI:
            result.append(["cargo", "build", *packages(pki), "--target", "thumbv7em-none-eabi"])
        return result
    if profile == "semver":
        return [["cargo", "semver-checks", "-p", "bitsandbytes", "--baseline-version", bnb["baseline"],
                 "--release-type", bnb["release_type"], feature]
                for feature in ("--all-features", "--default-features", "--only-explicit-features")]
    if profile == "package":
        # Verifies both actual archives together; Cargo supplies the temporary local registry.
        return [["cargo", "package", "-p", "bitsandbytes-macros", "-p", "bitsandbytes", "--all-features"]]
    if profile in {"usdr", "rust-dsdcc"}:
        return [["cargo", "check", "--manifest-path", f"{profile}/Cargo.toml"]]
    if profile == "rust-skills":
        manifest = ["--manifest-path", "tools/rust-skills/Cargo.toml"]
        return [["cargo", "fmt", *manifest, "--all", "--", "--check"],
                ["cargo", "clippy", *manifest, "--workspace", "--all-targets", "--", "-D", "warnings"],
                ["cargo", "test", *manifest, "--workspace"],
                ["cargo", "run", *manifest, "--package", "xtask", "--", "validate"],
                ["cargo", "run", *manifest, "--package", "xtask", "--", "generate", "--check"],
                ["cargo", "+1.85.0", "check", *manifest, "--workspace"]]
    raise ValueError(f"profile requires a dedicated adapter: {profile}")


def execute(arguments):
    print("+ " + shlex.join(arguments), flush=True)
    environment = os.environ.copy()
    if arguments[:2] == ["cargo", "doc"]:
        environment["RUSTDOCFLAGS"] = "-D warnings"
    subprocess.run(arguments, check=True, env=environment,
                   stdout=subprocess.DEVNULL if arguments[:2] == ["cargo", "metadata"] else None)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile", choices=sorted(PROFILES | {"fuzz"}))
    args = parser.parse_args()
    plan = json.loads(os.environ["CI_PLAN"])
    validate_plan(plan)
    if args.profile == "fuzz":
        item = json.loads(os.environ["FUZZ_ITEM"])
        if item not in plan["fuzz"]:
            raise ValueError("fuzz target was not selected")
        root, target = item["root"], item["target"]
        execute(["cargo", "metadata", "--manifest-path", f"{root}/Cargo.toml", "--locked", "--format-version", "1"])
        flags = ["--fuzz-dir", root, "--target", "x86_64-unknown-linux-gnu"]
        execute(["cargo", "fuzz", "build", target, *flags])
        execute(["cargo", "fuzz", "run", target, *flags, "--", *item["args"]])
        return
    if args.profile not in plan["profiles"]:
        raise ValueError("validation profile was not selected")
    if args.profile == "public-api":
        snapshot = Path("bitsandbytes/bnb/public-api.txt")
        generated = subprocess.run(["cargo", "public-api", "-p", "bitsandbytes", "--all-features"],
                                   check=True, stdout=subprocess.PIPE).stdout
        if generated != snapshot.read_bytes():
            candidate = Path(os.environ["RUNNER_TEMP"]) / "public-api.txt"
            candidate.write_bytes(generated)
            execute(["diff", "-u", str(snapshot), str(candidate)])
    elif args.profile == "blessed":
        snapshot = Path("rsl-deps/blessed-versions.toml")
        before = snapshot.read_bytes()
        execute([sys.executable, "tools/gen-blessed.py"])
        if snapshot.read_bytes() != before:
            raise ValueError("blessed-versions.toml is stale; regenerate and commit it")
    else:
        bnb = tomllib.loads(Path(".github/ci/bnb.toml").read_text())
        sequence = commands(args.profile, plan, bnb)
        if not sequence:
            raise ValueError("selected validation profile has no commands")
        for arguments in sequence:
            execute(arguments)


if __name__ == "__main__":
    try:
        main()
    except (KeyError, ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"CI validation failed: {error}", file=sys.stderr)
        sys.exit(1)
