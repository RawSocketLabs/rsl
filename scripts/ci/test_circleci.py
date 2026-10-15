"""CircleCI adapter tests: event mapping and the continued pipeline's job selection."""

import json
from pathlib import Path
import re
import tomllib
import unittest
from unittest import mock

import circleci
from circleci import continuation, event_options
from gate import coverage_report
from plan import handed_on
import test_plan

BNB_GATE = {"public_api_toolchain": "nightly-2026-06-17", "public_api_version": "0.52.0",
            "semver_version": "0.50.0"}
PREFIX = "release-plz-"
ROOT = Path(__file__).resolve().parents[2]


def options(env, merge_base="c" * 40):
    with mock.patch.object(circleci, "merge_base", return_value=merge_base):
        return event_options(None, env, PREFIX)


def cont(plan, gate, env):
    return continuation(plan, gate, env, "push" if env.get("CIRCLE_BRANCH") == "main" else "pull_request")


def run_commands(job):
    return [step["run"]["command"] for step in job["steps"] if isinstance(step, dict) and "run" in step]


class EventTests(unittest.TestCase):
    def test_trunk_push_compares_against_the_previous_pipeline(self):
        self.assertEqual(options({"CIRCLE_BRANCH": "main", "CI_BASE_REVISION": "a" * 40}),
                         ("push", "a" * 40, False, True, None))

    def test_an_absent_previous_revision_is_no_base(self):
        for revision in ("<nil>", "", "main", "a" * 39):
            with self.subTest(revision=revision):
                self.assertEqual(options({"CIRCLE_BRANCH": "main", "CI_BASE_REVISION": revision})[1:3], ("", True))

    def test_first_trunk_pipeline_without_a_previous_revision_is_full(self):
        self.assertTrue(options({"CIRCLE_BRANCH": "main", "CI_BASE_REVISION": ""})[2])

    def test_branch_compares_from_its_merge_base_with_the_trunk(self):
        event, base, full, release, _ = options({"CIRCLE_BRANCH": "feat/x", "CI_BASE_REVISION": "a" * 40})
        self.assertEqual((event, base, full, release), ("pull_request", "c" * 40, False, False))

    def test_gate_reuses_the_plan_jobs_merge_base(self):
        self.assertEqual(options({"CIRCLE_BRANCH": "feat/x", "CI_MERGE_BASE": "e" * 40})[1], "e" * 40)

    def test_branch_at_a_trunk_commit_is_full_not_an_empty_pass(self):
        self.assertTrue(options({"CIRCLE_BRANCH": "feat/x", "CIRCLE_SHA1": "c" * 40})[2])

    def test_comparison_base_must_be_a_revision(self):
        for base in ("-x", "a b", "$(id)"):
            with self.subTest(base=base), self.assertRaises(ValueError):
                options({"CIRCLE_BRANCH": "main", "CI_INPUT_BASE": base})

    def test_missing_trunk_history_is_full(self):
        self.assertTrue(options({"CIRCLE_BRANCH": "feat/x"}, merge_base="")[2])

    def test_only_same_repository_release_branches_qualify(self):
        self.assertTrue(options({"CIRCLE_BRANCH": PREFIX + "2026"})[3])
        self.assertFalse(options({"CIRCLE_BRANCH": PREFIX + "2026", "CIRCLE_PR_USERNAME": "fork"})[3])

    def test_schedule_passes_full_and_is_full_routine_coverage_without_release(self):
        self.assertEqual(options({"CIRCLE_BRANCH": "main", "CI_INPUT_FULL": "true"}),
                         ("workflow_dispatch", "", True, False, None))

    def test_parameters_are_a_manual_dispatch(self):
        self.assertEqual(options({"CIRCLE_BRANCH": "main", "CI_INPUT_PRODUCTS": "bitsandbytes",
                                  "CI_INPUT_RELEASE": "false"}),
                         ("workflow_dispatch", "", True, False, ["bitsandbytes"]))
        self.assertEqual(options({"CIRCLE_BRANCH": "feat/x", "CI_INPUT_BASE": "d" * 40})[:3],
                         ("workflow_dispatch", "d" * 40, False))

    def test_tag_pipelines_are_refused(self):
        with self.assertRaises(ValueError):
            options({"CIRCLE_TAG": "v1"})


class ReleaseAssociationTests(unittest.TestCase):
    def test_associated_pr_must_be_the_exact_release_merge(self):
        sha = "a" * 40
        pr = {"merged_at": "2026-09-12", "merge_commit_sha": sha,
              "head": {"ref": "release-plz-2026", "repo": {"full_name": circleci.REPOSITORY}},
              "base": {"ref": "main", "repo": {"full_name": circleci.REPOSITORY}}}
        self.assertTrue(circleci.is_release_merge(pr, sha, PREFIX))
        self.assertFalse(circleci.is_release_merge(pr, "b" * 40, PREFIX))
        self.assertFalse(circleci.is_release_merge({**pr, "merged_at": None}, sha, PREFIX))

    def test_release_association_requires_the_repository_and_prefix(self):
        pr = {"head": {"ref": "release-plz-2026-09-12", "repo": {"full_name": circleci.REPOSITORY}},
              "base": {"ref": "main", "repo": {"full_name": circleci.REPOSITORY}}}
        self.assertTrue(circleci.is_release_pr(pr, PREFIX))
        for replacement in ({"ref": "ci/affected-validation", "repo": {"full_name": circleci.REPOSITORY}},
                            {"ref": "release-plz-untrusted", "repo": {"full_name": "fork/rsl"}},
                            {"ref": "release-plz-deleted", "repo": None}):
            with self.subTest(head=replacement):
                self.assertFalse(circleci.is_release_pr({**pr, "head": replacement}, PREFIX))


class ContinuationTests(unittest.TestCase):
    def test_prose_schedules_only_policy_and_the_gate(self):
        config = cont(test_plan.planned(["README.md"]), BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        self.assertEqual(set(config["jobs"]), {"policy", "commitlint", "CI"})
        self.assertEqual(json.loads(config["jobs"]["CI"]["environment"]["CI_NEEDS"]),
                         {"checks": "skipped", "fuzz": "skipped"})

    def test_only_selected_profiles_and_fuzz_targets_are_scheduled_concurrently(self):
        plan = test_plan.planned(["protocols/session/socks/src/lib.rs"])
        config = cont(plan, BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        selected = {f"checks-{profile}" for profile in plan["profiles"] if profile != "workspace"}
        selected |= {f"checks-workspace-{part}" for part in ("lint", "test", "extra")} | {"commitlint"}
        self.assertEqual(set(config["jobs"]) - {"policy", "CI"}, selected)
        workflow = {name: spec for entry in config["workflows"]["ci"]["jobs"] for name, spec in entry.items()}
        for name in selected - {"commitlint"}:
            self.assertEqual(workflow[name], {"requires": ["policy"]})
        self.assertEqual(workflow["commitlint"], {})
        self.assertEqual(set(workflow["CI"]["requires"]), selected | {"policy"})

    def test_release_fuzz_targets_are_independent_jobs(self):
        plan = test_plan.planned(release=True)
        config = cont(plan, BNB_GATE, {"CIRCLE_BRANCH": "main"})
        fuzz = [name for name in config["jobs"] if name.startswith("fuzz-")]
        self.assertEqual(len(fuzz), len(plan["fuzz"]))
        self.assertTrue(fuzz)
        for name in fuzz:
            item = json.loads(config["jobs"][name]["environment"]["FUZZ_ITEM"])
            self.assertIn(item, plan["fuzz"])

    def test_jobs_are_handed_the_plan_without_its_detail(self):
        plan = test_plan.planned(["protocols/session/socks/src/lib.rs"])
        config = cont(plan, BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        for name, job in config["jobs"].items():
            if name != "commitlint":
                self.assertEqual(json.loads(job["environment"]["CI_PLAN"]), handed_on(plan))

    def test_continuation_declares_the_setup_parameters(self):
        config = cont(test_plan.planned(["README.md"]), BNB_GATE, {"CIRCLE_BRANCH": "main"})
        self.assertEqual(set(config["parameters"]), {"full", "base", "release", "products", "audit", "resource_class"})

    def test_every_job_runs_the_pinned_image_and_installs_nothing(self):
        config = cont(test_plan.planned(release=True), BNB_GATE, {"CIRCLE_BRANCH": "main"})
        for name, job in config["jobs"].items():
            if name == "commitlint":
                continue  # Runs the pinned upstream commitlint image through docker.
            with self.subTest(job=name):
                self.assertEqual(job["docker"], [{"image": circleci.IMAGE}])
                commands = " ".join(run_commands(job))
                for installer in ("rustup", "cargo install", "apt-get", "curl", "git clone"):
                    self.assertNotIn(installer, commands)

    def test_workspace_runs_as_three_concurrent_parts(self):
        config = cont(test_plan.planned(full=True), BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        self.assertNotIn("checks-workspace", config["jobs"])
        for part in ("lint", "test", "extra"):
            job = config["jobs"][f"checks-workspace-{part}"]
            self.assertEqual((job["environment"]["PROFILE"], job["environment"]["PART"]), ("workspace", part))
            self.assertIn('--part "$PART"', " ".join(run_commands(job)))
            self.assertEqual(any("store_test_results" in step for step in job["steps"] if isinstance(step, dict)),
                             part == "test")

    def test_only_the_trunk_saves_caches(self):
        plan = test_plan.planned(["protocols/session/socks/src/lib.rs"])
        for branch, saves in (("main", True), ("feat/x", False)):
            config = cont(plan, BNB_GATE, {"CIRCLE_BRANCH": branch})
            steps = config["jobs"]["checks-workspace-test"]["steps"]
            self.assertEqual(any("save_cache" in step for step in steps if isinstance(step, dict)), saves)

    def test_profiles_select_the_image_toolchains(self):
        jobs = cont(test_plan.planned(release=True), BNB_GATE, {"CIRCLE_BRANCH": "main"})["jobs"]
        self.assertEqual(jobs["checks-msrv"]["environment"]["RUSTUP_TOOLCHAIN"], "1.85.0")
        self.assertEqual(jobs["checks-public-api"]["environment"]["RUSTUP_TOOLCHAIN"], "nightly-2026-06-17")
        self.assertEqual(jobs["checks-workspace-lint"]["environment"]["RUSTUP_TOOLCHAIN"], circleci.STABLE)
        fuzz = next(job for name, job in jobs.items() if name.startswith("fuzz-"))
        self.assertEqual(fuzz["environment"]["RUSTUP_TOOLCHAIN"], "nightly-2026-06-17")

    def test_self_hosted_class_runs_the_same_image(self):
        jobs = cont(test_plan.planned(full=True), BNB_GATE,
                            {"CIRCLE_BRANCH": "main", "CI_INPUT_CLASS": "rsl/beefy"})["jobs"]
        self.assertEqual(jobs["checks-workspace-test"]["resource_class"], "rsl/beefy")
        self.assertEqual(jobs["checks-workspace-test"]["docker"], [{"image": circleci.IMAGE}])


    def test_gate_needs_produce_a_receipt_for_the_handed_plan(self):
        plan = test_plan.planned(["protocols/session/socks/src/lib.rs"])
        config = cont(plan, BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        needs = {"plan": {"result": "success"}, "policy": {"result": "success"},
                 **{job: {"result": result}
                    for job, result in json.loads(config["jobs"]["CI"]["environment"]["CI_NEEDS"]).items()}}
        report = coverage_report(handed_on(plan), needs, plan, event="pull_request", run_id="w", attempt=1)
        self.assertEqual(report["plan"], plan)


class ImageTests(unittest.TestCase):
    """The image, its users and the selector's tool pins must not drift apart."""

    def setUp(self):
        self.dockerfile = (ROOT / ".circleci/image/Dockerfile").read_text()

    def arg(self, name):
        return re.search(rf"^ARG {name}=(\S+)$", self.dockerfile, re.M).group(1)

    def test_setup_config_and_generator_pin_the_same_image(self):
        self.assertIn(f"- image: {circleci.IMAGE}\n", (ROOT / ".circleci/config.yml").read_text())
        self.assertRegex(circleci.IMAGE, r"^ghcr\.io/rawsocketlabs/rsl-ci@sha256:")

    def test_image_provides_the_toolchains_jobs_select(self):
        bnb = tomllib.loads((ROOT / ".github/ci/bnb.toml").read_text())
        self.assertEqual(self.arg("STABLE"), circleci.STABLE)
        self.assertEqual(self.arg("MSRV"), circleci.MSRV)
        self.assertEqual(self.arg("NIGHTLY"), bnb["public_api_toolchain"])
        self.assertEqual(self.arg("PUBLIC_API"), bnb["public_api_version"])
        self.assertEqual(self.arg("SEMVER"), bnb["semver_version"])

    def test_msrv_matches_the_workspace_floor(self):
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertEqual(manifest["workspace"]["package"]["rust-version"] + ".0", circleci.MSRV)


if __name__ == "__main__":
    unittest.main()
