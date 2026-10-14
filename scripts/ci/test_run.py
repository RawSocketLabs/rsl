"""Command-contract tests: preserve gate coverage while limiting package selection."""

from pathlib import Path
import sys
import subprocess
import socket
import tempfile
import textwrap
import unittest

from run import commands, locked_command
from plan import (FRONTEND, LIMESDR_PORTABLE, LMS7002M, LOWLEVEL, LP8758, LSDR, LSDR_PORTABLE,
                  PE_SYNC_PORTABLE, USDR,
                  USDR_PORTABLE, XSDR, XSDR_PORTABLE, handed_on)
import test_plan


def planned(*args, **kwargs):
    """A plan as the check jobs are handed it, so a key run.py reads cannot hide in the detail."""
    return handed_on(test_plan.planned(*args, **kwargs))


BNB_CONFIG = {"baseline": "0.6.0", "release_type": "patch"}


class ProfileTests(unittest.TestCase):
    def test_root_compilation_cannot_change_resolution_after_impact_selection(self):
        plan = planned(full=True, release=True)
        for profile in ("workspace", "network", "facades", "msrv", "no-std", "bnb", "socks"):
            for arguments in commands(profile, plan, BNB_CONFIG):
                if arguments[0] == "cargo" and "--manifest-path" not in arguments:
                    with self.subTest(profile=profile, arguments=arguments):
                        actual = locked_command(arguments)
                        if arguments[1] == "fmt":
                            self.assertEqual(actual, arguments)
                        else:
                            self.assertEqual(actual.count("--locked"), 1)
        detached = ["cargo", "check", "--manifest-path", "bindings/usdr/Cargo.toml"]
        self.assertEqual(locked_command(detached), detached)
        pinned = ["cargo", "+1.85.0", "check", "-p", "socks"]
        self.assertEqual(locked_command(pinned), ["cargo", "+1.85.0", "check", "--locked", "-p", "socks"])
        already = ["cargo", "test", "--locked", "-p", "socks"]
        self.assertEqual(locked_command(already), already)
    def test_optional_adapters_are_compiled_and_tested_without_enabling_release_ladders(self):
        for path, name in (("protocols/session/socks/src/mio.rs", "socks"),
                           ("codecs/bitsandbytes/bnb/src/io.rs", "bitsandbytes")):
            with self.subTest(package=name):
                plan = planned([path])
                sequence = commands("workspace", plan, BNB_CONFIG)
                self.assertIn(["cargo", "clippy", "-p", name, "--all-targets",
                               "--all-features", "--", "-D", "warnings"], sequence)
                self.assertIn(["cargo", "test", "-p", name, "--all-features"], sequence)
                self.assertNotIn("socks" if name == "socks" else "bnb", plan["profiles"])
                self.assertEqual(plan["fuzz"], [])

    def test_full_local_release_still_discovers_the_version_comparison_base(self):
        source = Path(__file__).resolve().parents[2] / "scripts/ci-act.sh"
        with tempfile.TemporaryDirectory(prefix="rsl-ci-cli-") as directory:
            root = Path(directory)
            scripts = root / "scripts"
            scripts.mkdir()
            entry = scripts / "ci-act.sh"
            entry.write_text(source.read_text())
            (root / ".actrc").write_text("-P ubuntu-latest=fixture\n")
            boundary = """
set -euo pipefail
podman() {
  case "$1" in
    info) printf '%s\\n' "$CI_SOCKET" ;;
    image) [[ "$2" != inspect ]] || printf '123456789012fixture\\n' ;;
    *) return 1 ;;
  esac
}
git() {
  case "$1" in
    status) ;;
    config) case "$3" in repoGuard.remote) printf 'origin\\n' ;; repoGuard.trunk) printf 'main\\n' ;; esac ;;
    merge-base) printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\\n' ;;
    symbolic-ref) printf 'release-plz-fixture\\n' ;;
    *) return 1 ;;
  esac
}
python3() { printf 'release-plz-\\n'; }
"""
            # PATH tools are test boundaries. The CLI only verifies that the API socket exists.
            tools = root / "bin"
            tools.mkdir()
            (tools / "act").write_text('#!/usr/bin/env bash\nset -euo pipefail\nprintf "%s\\n" "$@"\n')
            (tools / "act").chmod(0o755)
            with socket.socket(socket.AF_UNIX) as api:
                api.bind(str(root / "podman.sock"))
                for mode in ("pre-push", "release"):
                    with self.subTest(mode=mode):
                        result = subprocess.run(
                            ["bash", "-c", boundary + 'export -f podman git python3; bash "$CI_ENTRY" "$CI_MODE" --full'],
                            check=True, capture_output=True, text=True,
                            env={"PATH": str(tools) + ":/usr/bin:/bin", "CI_SOCKET": str(root / "podman.sock"),
                                 "CI_ENTRY": str(entry), "CI_MODE": mode})
                        self.assertIn("base=" + "a" * 40, result.stdout)
                        self.assertIn("full=true", result.stdout)
                        self.assertIn("release=true", result.stdout)

    def test_blessed_action_downloads_current_and_historical_snapshot_paths(self):
        root = Path(__file__).resolve().parents[2]
        action = (root / ".github/actions/blessed-drift/action.yml").read_text()
        script = textwrap.dedent(action.split("      run: |\n", 1)[1])
        script = script.replace("${{ inputs.ref }}", "fixture")
        script = script.replace("${{ inputs.strict == 'true' && '--strict' || '' }}", "--strict")
        # HTTP is the boundary: emulate only the requested revision's available files.
        fixture = '''
set -euo pipefail
curl() {
  printf '%s\\n' "$2"
  case "$2" in
    */tools/check-drift.py) printf 'print("drift_checked")\\n' > "$4" ;;
    */deps/blessed-versions.toml) [[ "$layout" == deps ]] || return 22
      printf '[versions]\\n' > "$4" ;;
    */rsl-deps/blessed-versions.toml) [[ "$layout" == rsl-deps ]] || return 22
      printf '[versions]\\n' > "$4" ;;
    *) return 22 ;;
  esac
}
'''
        for layout in ("deps", "rsl-deps"):
            with self.subTest(layout=layout), tempfile.TemporaryDirectory(prefix="rsl-ci-drift-") as directory:
                result = subprocess.run(
                    ["bash", "-c", fixture + script], check=True, capture_output=True, text=True,
                    env={"PATH": "/usr/bin:/bin", "RUNNER_TEMP": directory, "layout": layout})
                self.assertIn("/deps/blessed-versions.toml", result.stdout)
                self.assertEqual("/rsl-deps/blessed-versions.toml" in result.stdout, layout == "rsl-deps")
                self.assertTrue(result.stdout.rstrip().endswith("drift_checked"))

    def test_scoped_default_tests_and_msrv_never_expand_to_the_workspace(self):
        plan = planned(["protocols/session/socks/src/lib.rs"], release=True)
        for profile in ("workspace", "msrv"):
            with self.subTest(profile=profile):
                for command in commands(profile, plan, BNB_CONFIG):
                    self.assertNotIn("--workspace", command)
                    if command[0] == "cargo" and "-p" in command:
                        self.assertIn(command[command.index("-p") + 1], plan["packages"])

    def test_full_mode_keeps_workspace_feature_unification_tests(self):
        sequence = commands("workspace", planned(full=True), BNB_CONFIG)
        self.assertIn(["cargo", "test", "--workspace"], sequence)
        self.assertIn(["cargo", "clippy", "--workspace", "--all-targets"], sequence)

    def test_full_routine_scope_cannot_expand_release_commands(self):
        scoped = planned(release=True)
        broad = planned(["Cargo.lock", "crypto/core/src/lib.rs"], release=True)
        for profile in ("msrv", "no-std", "package", "bnb", "socks", "semver"):
            if profile in scoped["profiles"]:
                with self.subTest(profile=profile):
                    self.assertEqual(commands(profile, scoped, BNB_CONFIG),
                                     commands(profile, broad, BNB_CONFIG))
        self.assertFalse(any("--workspace" in command
                             for command in commands("msrv", broad, BNB_CONFIG)))

    def test_usdr_transports_are_feature_checked_only_when_selected(self):
        for plan in (planned(["devices/sdr/usdr/src/transport/usb/bus.rs"]), planned(full=True)):
            sequence = commands("workspace", plan, BNB_CONFIG)
            self.assertIn(["cargo", "clippy", "-p", "usdr", "--all-targets",
                           "--all-features", "--", "-D", "warnings"], sequence)
            self.assertIn(["cargo", "test", "-p", "usdr", "--all-features"], sequence)
            self.assertNotIn("msrv", plan["profiles"])
        release = planned(release=True)
        self.assertIn(["cargo", "check", "-p", "usdr", "--all-features"],
                      commands("msrv", release, BNB_CONFIG))
        sequence = commands("workspace", planned(["protocols/session/socks/src/lib.rs"]), BNB_CONFIG)
        self.assertFalse(any("usdr" in command for command in sequence))

    def test_portable_routine_gate_is_locked_host_only_and_checks_manifest_drift(self):
        for profile, name, fixture in (("usdr-portable", "usdr", USDR_PORTABLE),
                                       ("xsdr-portable", "xsdr", XSDR_PORTABLE),
                                       ("lsdr-portable", "lsdr", LSDR_PORTABLE),
                                       ("limesdr-portable", "limesdr", LIMESDR_PORTABLE),
                                       ("pe-sync-portable", "pe-sync", PE_SYNC_PORTABLE)):
            with self.subTest(profile=profile):
                sequence = commands(profile, planned([f"{fixture}/src/lib.rs"]), BNB_CONFIG)
                self.assertIn([sys.executable, "scripts/ci/portable.py", name], sequence)
                self.assertEqual({row[1] for row in sequence if row[0] == "cargo"},
                                 {"fmt", "clippy", "check", "test"})
                for row in sequence:
                    self.assertNotIn("--target", row)
                    self.assertNotIn("--workspace", row)
                    if row[0] == "cargo":
                        self.assertIn(f"{fixture}/Cargo.toml", row)
                        if row[1] != "fmt":
                            self.assertIn("--locked", row)

    def test_bnb_release_does_not_qualify_the_usdr_bare_metal_target(self):
        plan = planned(release=True)
        self.assertIn(USDR_PORTABLE, plan["release_roots"])
        sequence = commands("no-std", plan, BNB_CONFIG)
        self.assertFalse(any(f"{USDR_PORTABLE}/Cargo.toml" in row for row in sequence))

    def test_explicit_usdr_release_qualifies_its_portable_target(self):
        plan = planned([f"{USDR}/src/portable/temperature.rs"])
        plan["release_roots"] = [USDR, USDR_PORTABLE]
        plan["release_members"] = [{"root": USDR}]
        self.assertIn(["cargo", "check", "--manifest-path", f"{USDR_PORTABLE}/Cargo.toml",
                       "--locked", "--target", "thumbv7em-none-eabi"],
                      commands("no-std", plan, BNB_CONFIG))

    def test_shared_device_crates_get_a_bare_metal_check_only_from_a_usdr_release(self):
        bare = ["cargo", "check", "-p", "lp8758", "-p", "rsl-lowlevel",
                "--target", "thumbv7em-none-eabi"]
        codec = planned(release=True)
        self.assertIn(LP8758, codec["release_roots"])
        self.assertFalse(any("lp8758" in row for row in commands("no-std", codec, BNB_CONFIG)))
        plan = planned([f"{LOWLEVEL}/src/lib.rs"])
        plan["release_roots"] = [USDR, USDR_PORTABLE, LOWLEVEL, LP8758]
        plan["release_members"] = [{"root": USDR}]
        sequence = commands("no-std", plan, BNB_CONFIG)
        self.assertIn(bare, sequence)
        self.assertNotIn("--features", bare)
        self.assertEqual(locked_command(bare).count("--locked"), 1)

    def test_an_explicit_xsdr_release_checks_its_core_and_the_lms7002m_bare_metal(self):
        plan = planned([f"{LMS7002M}/src/lib.rs"])
        self.assertNotIn("no-std", plan["profiles"], "routine changes stay host-only")
        plan["release_roots"] = [XSDR, XSDR_PORTABLE, LOWLEVEL, LP8758, LMS7002M]
        plan["release_members"] = [{"root": XSDR}]
        sequence = commands("no-std", plan, BNB_CONFIG)
        self.assertIn(["cargo", "check", "--manifest-path", f"{XSDR_PORTABLE}/Cargo.toml",
                       "--locked", "--target", "thumbv7em-none-eabi"], sequence)
        self.assertIn(["cargo", "check", "-p", "lms7002m", "-p", "lp8758",
                       "-p", "rsl-lowlevel", "--target", "thumbv7em-none-eabi"], sequence)
        self.assertFalse(any(f"{USDR_PORTABLE}/Cargo.toml" in row for row in sequence))

    def test_an_explicit_lsdr_release_checks_its_core_on_bare_metal(self):
        plan = planned([f"{LSDR}/src/portable/probes.rs"])
        self.assertNotIn("no-std", plan["profiles"], "routine changes stay host-only")
        plan["release_roots"] = [LSDR, LSDR_PORTABLE, LOWLEVEL]
        plan["release_members"] = [{"root": LSDR}]
        sequence = commands("no-std", plan, BNB_CONFIG)
        self.assertIn(["cargo", "check", "--manifest-path", f"{LSDR_PORTABLE}/Cargo.toml",
                       "--locked", "--target", "thumbv7em-none-eabi"], sequence)
        self.assertIn(["cargo", "check", "-p", "rsl-lowlevel", "--target", "thumbv7em-none-eabi"],
                      sequence)
        self.assertFalse(any(f"{XSDR_PORTABLE}/Cargo.toml" in row for row in sequence))

    def test_the_front_end_crate_gets_its_bare_metal_check_without_std(self):
        plan = planned([f"{FRONTEND}/src/gpio.rs"])
        self.assertNotIn("no-std", plan["profiles"], "routine changes stay host-only")
        plan["release_roots"] = [LSDR, LSDR_PORTABLE, LOWLEVEL, FRONTEND]
        plan["release_members"] = [{"root": LSDR}]
        self.assertIn(["cargo", "check", "-p", "rsl-lowlevel", "-p", "sdr-frontend",
                       "--target", "thumbv7em-none-eabi"],
                      commands("no-std", plan, BNB_CONFIG))

    def test_bnb_feature_ladder_and_denied_warning_lints_are_retained(self):
        sequence = commands("bnb", planned(release=True), BNB_CONFIG)
        for feature in ("bytes", "mock", "tokio-io"):
            with self.subTest(feature=feature):
                self.assertIn(["cargo", "test", "-p", "bitsandbytes", "--features", feature], sequence)
        self.assertIn(["cargo", "test", "-p", "bitsandbytes", "--all-features"], sequence)
        self.assertTrue(any(command[:2] == ["cargo", "clippy"] and command[-2:] == ["-D", "warnings"]
                            for command in sequence))

    def test_socks_independent_features_and_interoperability_are_retained(self):
        sequence = commands("socks", planned(release=True), BNB_CONFIG)
        for feature in ("blocking", "tokio", "mio"):
            with self.subTest(feature=feature):
                self.assertIn(["cargo", "test", "-p", "socks", "--features", feature], sequence)
        self.assertIn(["cargo", "test", "-p", "socks", "--all-features"], sequence)

    def test_semver_checks_all_three_feature_modes(self):
        sequence = commands("semver", planned(release=True), BNB_CONFIG)
        self.assertEqual({command[-1] for command in sequence},
                         {"--all-features", "--default-features", "--only-explicit-features"})
        self.assertTrue(all(command[command.index("--baseline-version") + 1] == "0.6.0" for command in sequence))

    def test_compression_only_builds_its_affected_facade(self):
        sequence = commands("facades", planned(["codecs/compression/src/lib.rs"]), BNB_CONFIG)
        self.assertEqual(sequence, [["cargo", "build", "-p", "rsl", "--features", "full"]])

    def test_package_verification_never_uploads_or_disables_verification(self):
        sequence = commands("package", planned(release=True), BNB_CONFIG)
        self.assertEqual(sequence, [["cargo", "package", "-p", "bitsandbytes", "-p", "bitsandbytes-macros", "--all-features"]])

    def test_ffi_profiles_check_the_relocated_manifests(self):
        for profile, manifest in (("usdr-ffi", "bindings/usdr/Cargo.toml"),
                                  ("rust-dsdcc", "bindings/dsdcc/Cargo.toml")):
            with self.subTest(profile=profile):
                sequence = commands(profile, planned(full=True), BNB_CONFIG)
                self.assertEqual(sequence, [["cargo", "check", "--manifest-path", manifest]])

    def test_bladerf_compiles_all_targets_but_runs_only_hardware_free_tests(self):
        sequence = commands("bladerf", planned(full=True), BNB_CONFIG)
        manifest = ["--manifest-path", "bindings/bladerf/Cargo.toml"]
        self.assertIn(["cargo", "check", *manifest, "--workspace", "--all-targets"], sequence)
        self.assertIn(["cargo", "fmt", *manifest, "--all", "--", "--check"], sequence)
        self.assertIn(["cargo", "clippy", *manifest, "--workspace", "--all-targets",
                       "--", "-D", "warnings"], sequence)
        tests = [command for command in sequence if command[:2] == ["cargo", "test"]]
        self.assertEqual(tests, [["cargo", "test", *manifest, "-p", "bladerf", "--lib", "configure_module_"]])


if __name__ == "__main__":
    unittest.main()
