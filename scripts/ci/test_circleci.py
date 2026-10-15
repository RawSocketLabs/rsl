"""CircleCI adapter tests: event mapping and the continued pipeline's job selection."""

import json
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


def options(env, merge_base="c" * 40):
    with mock.patch.object(circleci, "merge_base", return_value=merge_base):
        return event_options(None, env, PREFIX)


def run_commands(job):
    return [step["run"]["command"] for step in job["steps"] if isinstance(step, dict) and "run" in step]


class EventTests(unittest.TestCase):
    def test_trunk_push_compares_against_the_previous_pipeline(self):
        self.assertEqual(options({"CIRCLE_BRANCH": "main", "CI_BASE_REVISION": "a" * 40}),
                         ("push", "a" * 40, False, False, None))

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

    def test_schedule_is_full_routine_coverage_without_release(self):
        self.assertEqual(options({"CIRCLE_BRANCH": "main", "CI_TRIGGER_SOURCE": "scheduled_pipeline"}),
                         ("schedule", "", True, False, None))

    def test_parameters_are_a_manual_dispatch(self):
        self.assertEqual(options({"CIRCLE_BRANCH": "main", "CI_INPUT_PRODUCTS": "bitsandbytes",
                                  "CI_INPUT_RELEASE": "false"}),
                         ("workflow_dispatch", "", True, False, ["bitsandbytes"]))
        self.assertEqual(options({"CIRCLE_BRANCH": "feat/x", "CI_INPUT_BASE": "d" * 40})[:3],
                         ("workflow_dispatch", "d" * 40, False))

    def test_products_are_refused_outside_manual_qualification(self):
        with self.assertRaises(ValueError):
            options({"CIRCLE_BRANCH": "main", "CI_TRIGGER_SOURCE": "scheduled_pipeline",
                     "CI_INPUT_PRODUCTS": "bitsandbytes"})

    def test_tag_pipelines_are_refused(self):
        with self.assertRaises(ValueError):
            options({"CIRCLE_TAG": "v1"})


class ContinuationTests(unittest.TestCase):
    def test_prose_schedules_only_policy_and_the_gate(self):
        config = continuation(test_plan.planned(["README.md"]), BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        self.assertEqual(set(config["jobs"]), {"policy", "CI"})
        self.assertEqual(json.loads(config["jobs"]["CI"]["environment"]["CI_NEEDS"]),
                         {"checks": "skipped", "fuzz": "skipped"})

    def test_only_selected_profiles_and_fuzz_targets_are_scheduled_concurrently(self):
        plan = test_plan.planned(["protocols/session/socks/src/lib.rs"])
        config = continuation(plan, BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        selected = {f"checks-{profile}" for profile in plan["profiles"] if profile != "workspace"}
        selected |= {f"checks-workspace-{part}" for part in ("lint", "test", "extra")}
        self.assertEqual(set(config["jobs"]) - {"policy", "CI"}, selected)
        workflow = {name: spec for entry in config["workflows"]["ci"]["jobs"] for name, spec in entry.items()}
        for name in selected:
            self.assertEqual(workflow[name], {"requires": ["policy"]})
        self.assertEqual(set(workflow["CI"]["requires"]), selected | {"policy"})

    def test_release_fuzz_targets_are_independent_jobs(self):
        plan = test_plan.planned(release=True)
        config = continuation(plan, BNB_GATE, {"CIRCLE_BRANCH": "main"})
        fuzz = [name for name in config["jobs"] if name.startswith("fuzz-")]
        self.assertEqual(len(fuzz), len(plan["fuzz"]))
        self.assertTrue(fuzz)
        for name in fuzz:
            item = json.loads(config["jobs"][name]["environment"]["FUZZ_ITEM"])
            self.assertIn(item, plan["fuzz"])

    def test_jobs_are_handed_the_plan_without_its_detail(self):
        plan = test_plan.planned(["protocols/session/socks/src/lib.rs"])
        config = continuation(plan, BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        for job in config["jobs"].values():
            self.assertEqual(json.loads(job["environment"]["CI_PLAN"]), handed_on(plan))

    def test_continuation_declares_the_setup_parameters(self):
        config = continuation(test_plan.planned(["README.md"]), BNB_GATE, {"CIRCLE_BRANCH": "main"})
        self.assertEqual(set(config["parameters"]), {"full", "base", "release", "products", "resource_class"})

    def test_cargo_tools_are_cached_in_their_own_roots(self):
        config = continuation(test_plan.planned(release=True), BNB_GATE, {"CIRCLE_BRANCH": "main"})
        for job in config["jobs"].values():
            for step in job["steps"]:
                if isinstance(step, dict) and "save_cache" in step:
                    self.assertNotIn("~/.cargo/bin", step["save_cache"]["paths"])

    def test_workspace_runs_as_three_concurrent_parts(self):
        config = continuation(test_plan.planned(full=True), BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        self.assertNotIn("checks-workspace", config["jobs"])
        for part in ("lint", "test", "extra"):
            job = config["jobs"][f"checks-workspace-{part}"]
            self.assertEqual((job["environment"]["PROFILE"], job["environment"]["PART"]), ("workspace", part))
            self.assertIn('--part "$PART"', " ".join(run_commands(job)))
            self.assertEqual("nextest" in " ".join(run_commands(job)), part == "test")

    def test_only_the_trunk_saves_caches(self):
        plan = test_plan.planned(["protocols/session/socks/src/lib.rs"])
        for branch, saves in (("main", True), ("feat/x", False)):
            config = continuation(plan, BNB_GATE, {"CIRCLE_BRANCH": branch})
            steps = config["jobs"]["checks-workspace-test"]["steps"]
            self.assertEqual(any("save_cache" in step for step in steps if isinstance(step, dict)), saves)

    def test_profile_setup_matches_the_actions_workflow(self):
        plan = test_plan.planned(release=True)
        config = continuation(plan, BNB_GATE, {"CIRCLE_BRANCH": "main"})
        jobs = config["jobs"]
        self.assertIn("rustfmt,clippy,rust-src", " ".join(run_commands(jobs["checks-workspace-lint"])))
        self.assertEqual(jobs["checks-msrv"]["environment"]["RUSTUP_TOOLCHAIN"], "1.85.0")
        self.assertEqual(jobs["checks-public-api"]["environment"]["RUSTUP_TOOLCHAIN"], "nightly-2026-06-17")
        self.assertIn("cargo-public-api 0.52.0", " ".join(run_commands(jobs["checks-public-api"])))

    def test_self_hosted_class_skips_native_package_installation(self):
        plan = test_plan.planned(full=True)
        hosted = continuation(plan, BNB_GATE, {"CIRCLE_BRANCH": "main"})["jobs"]
        runner = continuation(plan, BNB_GATE, {"CIRCLE_BRANCH": "main", "CI_INPUT_CLASS": "rsl/beefy"})["jobs"]
        self.assertIn("apt", " ".join(run_commands(hosted["checks-usdr-oracle"])))
        self.assertNotIn("apt", " ".join(run_commands(runner["checks-usdr-oracle"])))
        self.assertEqual((runner["checks-workspace-test"]["machine"], runner["checks-workspace-test"]["resource_class"]),
                         (True, "rsl/beefy"))

    def test_gate_needs_produce_a_receipt_for_the_handed_plan(self):
        plan = test_plan.planned(["protocols/session/socks/src/lib.rs"])
        config = continuation(plan, BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        needs = {"plan": {"result": "success"}, "policy": {"result": "success"},
                 **{job: {"result": result}
                    for job, result in json.loads(config["jobs"]["CI"]["environment"]["CI_NEEDS"]).items()}}
        report = coverage_report(handed_on(plan), needs, plan, event="pull_request", run_id="w", attempt=1)
        self.assertEqual(report["plan"], plan)


if __name__ == "__main__":
    unittest.main()
