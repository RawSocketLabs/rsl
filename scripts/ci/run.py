#!/usr/bin/env python3
"""Fixed validation profiles shared by hosted Actions and local act execution.

The plan selects routine packages and independent release scope, not arbitrary shell commands.
Preserve complete selected suites;
dependency builds are expected, but unrelated dependency test suites are not.
"""

import argparse
import json
import os
from pathlib import Path, PurePosixPath
import shlex
import subprocess
import sys
import tomllib

from gate import validate_plan
from plan import (BARE_METAL_DEVICES, BLADERF, BNB, LIMESDR_ORACLE, LSDR_ORACLE, NOSTD,
                  PE_SYNC_ORACLE, PKI, PORTABLE_FIXTURES, PROFILES, USDR_ORACLE, XSDR_ORACLE,
                  bare_metal_devices, released_portable_drivers)


def packages(names):
    return [argument for name in sorted(names) for argument in ("-p", name)]


def workspace_parts(plan):
    """The workspace profile as independent parts, so a runner can give each its own machine.

    `test` runs every test binary concurrently through nextest, plus the doctests nextest does
    not run; together they are exactly `cargo test` over the same packages and features."""
    selected = set(plan["packages"])
    workspace = ["--workspace"] if plan["full"] else packages(selected)
    # One representative configuration keeps optional adapters/transport code in
    # the routine gate; independent feature ladders and docs stay release-only.
    adapters = sorted(selected & {"bitsandbytes", "socks", "usdr"})
    return {
        "lint": [["cargo", "fmt", "--all", "--check"],
                 ["cargo", "clippy", *workspace, "--all-targets"],
                 *(["cargo", "clippy", "-p", name, "--all-targets", "--all-features", "--", "-D", "warnings"]
                   for name in adapters)],
        "test": [["cargo", "nextest", "run", "--locked", "--no-tests=warn", *workspace],
                 ["cargo", "test", "--doc", *workspace]],
        "extra": [*(["cargo", "test", "-p", name, "--all-features"] for name in adapters),
                  [sys.executable, "-m", "unittest", "discover", "-s", "scripts/ci", "-p", "integration_*.py", "-v"]],
    }


def commands(profile, plan, bnb):
    selected = set(plan["packages"])
    roots = set(plan["roots"])
    qualified = set(plan["release_packages"])
    release_roots = set(plan["release_roots"])
    workspace = ["--workspace"] if plan["full"] else packages(selected)
    if profile == "workspace":
        parts = workspace_parts(plan)
        return [*parts["lint"], ["cargo", "test", *workspace], *parts["extra"]]
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
                ["cargo", "test", "-p", "socks", "--features", "mio"],
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
                  for name in sorted(selected & {"rsl", "deps"})]
        if "protocol-examples" in selected:
            result.append(["cargo", "build", "-p", "protocol-examples", "--examples"])
        return result
    if profile == "msrv":
        result = [["cargo", "check", *packages(qualified)]] if qualified else []
        result.extend(["cargo", "check", "-p", name, "--all-features"]
                      for name in sorted(qualified & {"bitsandbytes", "socks", "usdr"}))
        if NOSTD in release_roots:
            result.append(["cargo", "check", "--manifest-path", f"{NOSTD}/Cargo.toml"])
        return result
    if profile == "no-std":
        result = []
        if BNB in release_roots:
            result.append(["cargo", "build", "-p", "bitsandbytes", "--no-default-features"])
        if NOSTD in release_roots:
            result.append(["cargo", "build", "--manifest-path", f"{NOSTD}/Cargo.toml",
                           "--target", "thumbv7em-none-eabi"])
        for driver in sorted(released_portable_drivers(plan["release_members"])):
            fixture = PORTABLE_FIXTURES[driver]
            if fixture in release_roots:
                result.append(["cargo", "check", "--manifest-path", f"{fixture}/Cargo.toml",
                               "--locked", "--target", "thumbv7em-none-eabi"])
        devices = bare_metal_devices(release_roots, plan["release_members"])
        if devices:
            # Default features only: `std` (hal's Bus) is the host driver's.
            result.append(["cargo", "check",
                           *packages(BARE_METAL_DEVICES[root] for root in devices),
                           "--target", "thumbv7em-none-eabi"])
        pki = qualified & {"asn1", "x509", "pki"}
        if release_roots & PKI:
            result.append(["cargo", "build", *packages(pki), "--target", "thumbv7em-none-eabi"])
        return result
    if profile == "semver":
        return [["cargo", "semver-checks", "-p", "bitsandbytes", "--baseline-version", bnb["baseline"],
                 "--release-type", bnb["release_type"], feature]
                for feature in ("--all-features", "--default-features", "--only-explicit-features")]
    if profile == "package":
        # Verifies both actual archives together; Cargo supplies the temporary local registry.
        return [["cargo", "package", *packages({member["name"] for member in plan["release_members"]}),
                 "--all-features"]]
    if profile == "bladerf":
        manifest = ["--manifest-path", f"{BLADERF}/Cargo.toml"]
        return [["cargo", "fmt", *manifest, "--all", "--", "--check"],
                ["cargo", "clippy", *manifest, "--workspace", "--all-targets", "--", "-D", "warnings"],
                ["cargo", "check", *manifest, "--workspace", "--all-targets"],
                ["cargo", "test", *manifest, "-p", "bladerf", "--lib", "configure_module_"]]
    if profile in {"usdr-ffi", "rust-dsdcc"}:
        directory = {"usdr-ffi": "usdr", "rust-dsdcc": "dsdcc"}[profile]
        return [["cargo", "check", "--manifest-path", f"bindings/{directory}/Cargo.toml"]]
    if profile in {"usdr-oracle", "xsdr-oracle", "lsdr-oracle", "pe-sync-oracle",
                   "limesdr-oracle"}:
        oracle = {"usdr-oracle": USDR_ORACLE, "xsdr-oracle": XSDR_ORACLE,
                  "lsdr-oracle": LSDR_ORACLE, "pe-sync-oracle": PE_SYNC_ORACLE,
                  "limesdr-oracle": LIMESDR_ORACLE}[profile]
        manifest = ["--manifest-path", f"{oracle}/Cargo.toml"]
        return [["cargo", "fmt", *manifest, "--", "--check"],
                ["cargo", "clippy", *manifest, "--locked", "--all-targets", "--", "-D", "warnings"],
                ["cargo", "test", *manifest, "--locked"]]
    # Profiles and portable.py fixtures are named for the driver directory, dashed
    # (`pe_sync` is `pe-sync-portable`).
    portable = {f"{PurePosixPath(driver).name.replace('_', '-')}-portable": fixture
                for driver, fixture in PORTABLE_FIXTURES.items()}
    if profile in portable:
        name = profile.removesuffix("-portable")
        manifest = ["--manifest-path", f"{portable[profile]}/Cargo.toml"]
        return [[sys.executable, "scripts/ci/portable.py", name],
                ["cargo", "fmt", *manifest, "--", "--check"],
                ["cargo", "clippy", *manifest, "--locked", "--all-targets", "--", "-D", "warnings"],
                ["cargo", "check", *manifest, "--locked"],
                ["cargo", "test", *manifest, "--locked"]]
    if profile == "rust-skills":
        manifest = ["--manifest-path", "tools/rust-skills/Cargo.toml"]
        return [["cargo", "fmt", *manifest, "--all", "--", "--check"],
                ["cargo", "clippy", *manifest, "--workspace", "--all-targets", "--", "-D", "warnings"],
                ["cargo", "test", *manifest, "--workspace"],
                ["cargo", "run", *manifest, "--package", "xtask", "--", "validate"],
                ["cargo", "run", *manifest, "--package", "xtask", "--", "generate", "--check"],
                *([["cargo", "+1.85.0", "check", *manifest, "--workspace"]]
                  if any(root.startswith("tools/rust-skills/") for root in release_roots) else [])]
    raise ValueError(f"profile requires a dedicated adapter: {profile}")


def locked_command(arguments):
    """Root compilation must not introduce resolutions outside the selected impact graph."""
    operation = 2 if len(arguments) > 1 and arguments[1].startswith("+") else 1
    if (arguments[0] == "cargo" and len(arguments) > operation
            and arguments[operation] in {"build", "check", "clippy", "test", "doc"}
            and "--manifest-path" not in arguments and "--locked" not in arguments):
        return [*arguments[:operation + 1], "--locked", *arguments[operation + 1:]]
    return arguments


def execute(arguments):
    arguments = locked_command(arguments)
    print("+ " + shlex.join(arguments), flush=True)
    environment = os.environ.copy()
    if arguments[:2] == ["cargo", "doc"]:
        environment["RUSTDOCFLAGS"] = "-D warnings"
    subprocess.run(arguments, check=True, env=environment,
                   stdout=subprocess.DEVNULL if arguments[:2] == ["cargo", "metadata"] else None)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile", choices=sorted(PROFILES | {"fuzz"}))
    parser.add_argument("--part", choices=("lint", "test", "extra"), help="one part of the workspace profile")
    args = parser.parse_args()
    if args.part and args.profile != "workspace":
        parser.error("--part applies only to the workspace profile")
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
        snapshot = Path("codecs/bitsandbytes/bnb/public-api.txt")
        generated = subprocess.run(["cargo", "public-api", "-p", "bitsandbytes", "--all-features"],
                                   check=True, stdout=subprocess.PIPE).stdout
        if generated != snapshot.read_bytes():
            candidate = Path(os.environ["RUNNER_TEMP"]) / "public-api.txt"
            candidate.write_bytes(generated)
            execute(["diff", "-u", str(snapshot), str(candidate)])
    elif args.profile == "blessed":
        snapshot = Path("deps/blessed-versions.toml")
        before = snapshot.read_bytes()
        execute([sys.executable, "tools/gen-blessed.py"])
        if snapshot.read_bytes() != before:
            raise ValueError("blessed-versions.toml is stale; regenerate and commit it")
    else:
        bnb = tomllib.loads(Path(".github/ci/bnb.toml").read_text())
        sequence = (workspace_parts(plan)[args.part] if args.part else
                    commands(args.profile, plan, bnb))
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
