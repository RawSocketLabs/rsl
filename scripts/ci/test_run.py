"""Command-contract tests: preserve gate coverage while limiting package selection."""

import unittest

from run import commands
from test_plan import planned

BNB_CONFIG = {"baseline": "0.6.0", "release_type": "patch"}


class ProfileTests(unittest.TestCase):
    def test_scoped_default_tests_and_msrv_never_expand_to_the_workspace(self):
        plan = planned(["protocols/session/socks/src/lib.rs"])
        for profile in ("workspace", "msrv"):
            with self.subTest(profile=profile):
                for command in commands(profile, plan, BNB_CONFIG):
                    self.assertNotIn("--workspace", command)
                    if command[0] == "cargo" and "-p" in command:
                        self.assertEqual(command[command.index("-p") + 1], "socks")

    def test_full_mode_keeps_workspace_feature_unification_tests(self):
        sequence = commands("workspace", planned(full=True), BNB_CONFIG)
        self.assertIn(["cargo", "test", "--workspace"], sequence)
        self.assertIn(["cargo", "clippy", "--workspace", "--all-targets"], sequence)

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
        for feature in ("blocking", "tokio"):
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
        self.assertEqual(sequence, [["cargo", "package", "-p", "bitsandbytes-macros", "-p", "bitsandbytes", "--all-features"]])


if __name__ == "__main__":
    unittest.main()
