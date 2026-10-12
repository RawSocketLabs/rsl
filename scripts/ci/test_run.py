"""Command-contract tests: preserve gate coverage while limiting package selection."""

from pathlib import Path
import subprocess
import socket
import tempfile
import textwrap
import unittest

from run import commands
from test_plan import planned

BNB_CONFIG = {"baseline": "0.6.0", "release_type": "patch"}


class ProfileTests(unittest.TestCase):
    def test_optional_adapters_are_compiled_and_tested_without_enabling_release_ladders(self):
        for path, name in (("protocols/session/socks/src/mio.rs", "socks"),
                           ("bitsandbytes/bnb/src/io.rs", "bitsandbytes")):
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
        broad = planned(["Cargo.lock", "crypto/src/lib.rs"], release=True)
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
            self.assertIn(["cargo", "clippy", "-p", "rsl-usdr", "--all-targets",
                           "--all-features", "--", "-D", "warnings"], sequence)
            self.assertIn(["cargo", "test", "-p", "rsl-usdr", "--all-features"], sequence)
            self.assertNotIn("msrv", plan["profiles"])
        release = planned(release=True)
        self.assertIn(["cargo", "check", "-p", "rsl-usdr", "--all-features"],
                      commands("msrv", release, BNB_CONFIG))
        sequence = commands("workspace", planned(["protocols/session/socks/src/lib.rs"]), BNB_CONFIG)
        self.assertFalse(any("rsl-usdr" in command for command in sequence))

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
        sequence = commands("facades", planned(["compression/src/lib.rs"]), BNB_CONFIG)
        self.assertEqual(sequence, [["cargo", "build", "-p", "rsl", "--features", "full"]])

    def test_package_verification_never_uploads_or_disables_verification(self):
        sequence = commands("package", planned(release=True), BNB_CONFIG)
        self.assertEqual(sequence, [["cargo", "package", "-p", "bitsandbytes", "-p", "bitsandbytes-macros", "--all-features"]])

    def test_ffi_profiles_check_the_relocated_manifests(self):
        for profile, manifest in (("usdr", "bindings/usdr/Cargo.toml"),
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
