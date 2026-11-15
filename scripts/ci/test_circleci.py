"""CircleCI adapter tests: event mapping and the continued pipeline's job selection."""

import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import tomllib
import unittest
from unittest import mock

import circleci
from circleci import continuation, event_options
from gate import coverage_report
from plan import PROFILES, handed_on
import test_plan

BNB_GATE = {"public_api_toolchain": "nightly-2026-06-17", "public_api_version": "0.52.0",
            "semver_version": "0.50.0"}
PREFIX = "release-plz-"
ROOT = Path(__file__).resolve().parents[2]


def options(env, merge_base="c" * 40, parent="", previous=""):
    with mock.patch.object(circleci, "merge_base", return_value=merge_base), \
            mock.patch.object(circleci, "first_parent", return_value=parent), \
            mock.patch.object(circleci, "previous_trunk_revision", return_value=previous):
        return event_options(None, env, PREFIX)


def pipelines(*items):
    response = mock.MagicMock()
    response.__enter__.return_value.read.return_value = json.dumps({"items": [
        {"number": number, "trigger_parameters": {"git": {"checkout_sha": sha}}} for number, sha in items]})
    return response


def cont(plan, gate, env):
    return continuation(plan, gate, env, "push" if env.get("CIRCLE_BRANCH") == "main" else "pull_request")


def run_commands(job):
    return [step["run"]["command"] for step in job["steps"] if isinstance(step, dict) and "run" in step]


class EventTests(unittest.TestCase):
    def test_trunk_push_compares_against_the_previous_pipeline(self):
        self.assertEqual(options({"CIRCLE_BRANCH": "main", "CI_BASE_REVISION": "a" * 40}),
                         ("push", "a" * 40, False, True, None))

    def test_an_absent_previous_revision_on_a_root_commit_is_full_without_a_base(self):
        for revision in ("<nil>", "", "main", "a" * 39):
            with self.subTest(revision=revision):
                self.assertEqual(options({"CIRCLE_BRANCH": "main", "CI_BASE_REVISION": revision})[1:3], ("", True))

    def test_an_absent_previous_revision_is_full_with_the_first_parent_as_release_base(self):
        for revision in ("<nil>", "", "e" * 40):
            with self.subTest(revision=revision):
                env = {"CIRCLE_BRANCH": "main", "CI_BASE_REVISION": revision, "CIRCLE_SHA1": "e" * 40}
                self.assertEqual(options(env, parent="d" * 40), ("push", "d" * 40, True, True, None))

    def test_an_absent_previous_revision_falls_back_to_the_previous_trunk_pipeline(self):
        env = {"CIRCLE_BRANCH": "main", "CI_BASE_REVISION": "<nil>", "CIRCLE_SHA1": "e" * 40}
        self.assertEqual(options(env, parent="d" * 40, previous="a" * 40), ("push", "a" * 40, False, True, None))
        # A re-run of the same commit still has no diff.
        self.assertEqual(options(env, parent="d" * 40, previous="e" * 40), ("push", "d" * 40, True, True, None))

    def test_the_gate_reads_the_pinned_trunk_base_not_the_network(self):
        env = {"CIRCLE_BRANCH": "main", "CI_BASE_REVISION": "<nil>", "CIRCLE_SHA1": "e" * 40}
        with mock.patch.object(circleci, "merge_base"), \
                mock.patch.object(circleci, "first_parent", return_value="d" * 40), \
                mock.patch.object(circleci, "previous_trunk_revision") as lookup, \
                mock.patch.object(circleci, "descends", return_value=True):
            self.assertEqual(event_options(None, {**env, "CI_TRUNK_BASE": "a" * 40}, PREFIX)[1:3], ("a" * 40, False))
            for pinned in ("none", "", "a" * 39):
                with self.subTest(pinned=pinned):
                    self.assertEqual(event_options(None, {**env, "CI_TRUNK_BASE": pinned}, PREFIX)[1:3],
                                     ("d" * 40, True))
            lookup.assert_not_called()
        with mock.patch.object(circleci, "first_parent", return_value="d" * 40), \
                mock.patch.object(circleci, "descends", return_value=False):
            self.assertEqual(event_options(None, {**env, "CI_TRUNK_BASE": "a" * 40}, PREFIX)[1:3], ("d" * 40, True))

    def test_the_continuation_hands_the_plan_jobs_trunk_base_to_the_gate(self):
        scoped = test_plan.planned(["protocols/session/socks/src/lib.rs"])
        for plan, env, pinned in ((scoped, {"CIRCLE_BRANCH": "main", "CI_TRUNK_BASE": "a" * 40}, "a" * 40),
                                  (test_plan.planned(full=True), {"CIRCLE_BRANCH": "main", "CI_TRUNK_BASE": "none"}, "none"),
                                  (scoped, {"CIRCLE_BRANCH": "feat/x"}, "none")):
            with self.subTest(env=env, full=plan["full"]):
                config = cont(plan, BNB_GATE, env)
                self.assertEqual(config["jobs"]["rsl"]["environment"]["CI_TRUNK_BASE"], pinned)

    def test_previous_trunk_revision_is_the_latest_earlier_green_pipeline_head_descends_from(self):
        env = {"CI_PIPELINE_NUMBER": "137", "CI_PROJECT_SLUG": "circleci/org/project"}
        history = pipelines((140, "f" * 40), (137, "e" * 40), (134, "a" * 40), (133, "b" * 40))
        with mock.patch.object(circleci.urllib.request, "urlopen", return_value=history) as urlopen, \
                mock.patch.object(circleci, "descends", return_value=True) as descends, \
                mock.patch.object(circleci, "gate_passed", return_value=True) as passed:
            self.assertEqual(circleci.previous_trunk_revision("root", env), "a" * 40)
        self.assertEqual(urlopen.call_args.args[0],
                         "https://circleci.com/api/v2/project/circleci/org/project/pipeline?branch=main")
        descends.assert_called_once_with("root", "a" * 40)
        passed.assert_called_once_with("a" * 40)

    def test_previous_trunk_revision_proves_nothing_unless_it_is_certain(self):
        env = {"CI_PIPELINE_NUMBER": "137", "CI_PROJECT_SLUG": "circleci/org/project"}
        earlier = pipelines((134, "a" * 40))
        cases = {
            "force-pushed away": ({}, earlier, False, True),
            "red or unfinished predecessor": ({}, earlier, True, False),
            "no earlier pipeline on the page": ({}, pipelines((140, "f" * 40)), True, True),
            "not a commit": ({}, pipelines((134, None)), True, True),
            "malformed history": ({}, mock.MagicMock(**{"__enter__.return_value.read.return_value": "{}"}), True, True),
            "no pipeline number": ({"CI_PIPELINE_NUMBER": ""}, earlier, True, True),
            "no project": ({"CI_PROJECT_SLUG": ""}, earlier, True, True),
        }
        for name, (overrides, history, descends, passed) in cases.items():
            with self.subTest(name), mock.patch.object(circleci.urllib.request, "urlopen", return_value=history), \
                    mock.patch.object(circleci, "descends", return_value=descends), \
                    mock.patch.object(circleci, "gate_passed", return_value=passed):
                self.assertEqual(circleci.previous_trunk_revision(None, {**env, **overrides}), "")
        with mock.patch.object(circleci.urllib.request, "urlopen", side_effect=OSError("unreachable")):
            self.assertEqual(circleci.previous_trunk_revision(None, env), "")

    def test_a_trunk_push_already_green_on_its_branch_re_tests_nothing(self):
        head = "e" * 40
        scoped = {**test_plan.planned(["codecs/bitsandbytes/bnb/src/lib.rs"]), "head": head}
        empty = {**test_plan.planned(), "base": head, "head": head}
        proven = ("a" * 40, False)  # (base, event-level full): a green trunk predecessor
        cases = {  # name: (event, (base, full), plan, tested, skipped)
            "tested push": ("push", proven, scoped, True, True),
            "untested push": ("push", proven, scoped, False, False),
            # Central CI makes the plan full; the branch's diff from the same base held it too.
            "full-by-content push": ("push", proven, {**scoped, "full": True, "paths": ["scripts/ci/plan.py"]},
                                     True, True),
            "push without a green predecessor": ("push", ("b" * 40, True), {**scoped, "full": True}, True, False),
            "full push changing a cache key": ("push", proven, {**scoped, "full": True, "paths": ["Cargo.lock"]},
                                               True, False),
            "release push": ("push", proven, {**scoped, "release": True}, True, False),
            "image push": ("push", proven, {**scoped, "paths": [".circleci/image/Dockerfile"]}, True, False),
            "branch": ("pull_request", proven, scoped, True, False),
        }
        for name, (event, (base, full), plan, tested, skipped) in cases.items():
            env = {"CIRCLE_SHA1": head}
            with self.subTest(name), \
                    mock.patch.object(circleci, "event_options", return_value=(event, base, full, True, None)), \
                    mock.patch.object(circleci, "release_policy", return_value=PREFIX), \
                    mock.patch.object(circleci, "validate_plan"), \
                    mock.patch.object(circleci, "build_plan", side_effect=[dict(plan), dict(empty)]) as build, \
                    mock.patch.object(circleci, "gate_passed", return_value=tested):
                _, result = circleci.plan_for_event(None, env)
                self.assertEqual(result["profiles"] == [] and result["base"] == head, skipped)
                if skipped:
                    self.assertEqual(build.call_args.kwargs, {"base": head})
                    # Pinned for the gate, which must reach the same plan without the network.
                    self.assertEqual((env["CI_HEAD_TESTED"], env["CI_TRUNK_BASE"]), ("true", "a" * 40))
                    jobs = cont(result, BNB_GATE, {"CIRCLE_BRANCH": "main", **env})["jobs"]
                    self.assertEqual(set(jobs), {"policy", "rsl", "release-pr"})
                    self.assertEqual(jobs["rsl"]["environment"]["CI_HEAD_TESTED"], "true")
                    # The gate's recomputation reads the pins, never the network.
                    with mock.patch.object(circleci, "build_plan", side_effect=[dict(plan), dict(empty)]), \
                            mock.patch.object(circleci, "gate_passed", side_effect=OSError("offline")):
                        self.assertEqual(circleci.plan_for_event(None, env)[1], result)

    def test_a_trunk_push_made_full_by_what_it_changed_keeps_its_base_for_the_gate(self):
        # Two commits, the first touching a global file: the plan is full, but against the
        # previous trunk revision, which is not this push's first parent. Then the same push with
        # no proven predecessor, where both sides fall back to the first parent.
        head, previous, parent = "e" * 40, "a" * 40, "d" * 40
        template = {**test_plan.planned(full=True), "head": head}

        def planned(_root, *, base, **_):
            return {**template, "base": base}

        for found, base, pin in ((previous, previous, previous), ("", parent, "none")):
            env = {"CIRCLE_BRANCH": "main", "CI_BASE_REVISION": "<nil>", "CIRCLE_SHA1": head}
            with self.subTest(previous=found), \
                    mock.patch.object(circleci, "release_policy", return_value=PREFIX), \
                    mock.patch.object(circleci, "validate_plan"), \
                    mock.patch.object(circleci, "build_plan", side_effect=planned), \
                    mock.patch.object(circleci, "first_parent", return_value=parent), \
                    mock.patch.object(circleci, "descends", return_value=True), \
                    mock.patch.object(circleci, "gate_passed", return_value=False):
                with mock.patch.object(circleci, "previous_trunk_revision", return_value=found):
                    _, handed = circleci.plan_for_event(None, env)
                self.assertEqual(handed["base"], base)
                # The gate has what the continuation handed its job and no network, and must
                # reach the plan it was handed.
                handed_on_env = cont(handed, BNB_GATE, env)["jobs"]["rsl"]["environment"]
                self.assertEqual(handed_on_env["CI_TRUNK_BASE"], pin)
                gate = {**handed_on_env, "CIRCLE_BRANCH": "main", "CIRCLE_SHA1": head}
                with mock.patch.object(circleci, "previous_trunk_revision", side_effect=OSError("offline")):
                    self.assertEqual(circleci.plan_for_event(None, gate)[1], handed)

    def test_reads_retry_lost_connections_and_server_errors_but_not_refusals(self):
        def http(code):
            return circleci.urllib.error.HTTPError("u", code, "", {}, None)
        lost = circleci.urllib.error.URLError("reset")
        for outcomes, calls, ok in (([lost, http(502), "ok"], 3, True), ([http(403)], 1, False),
                                    ([lost] * 3, 3, False)):
            with self.subTest(outcomes=outcomes), mock.patch.object(circleci.time, "sleep"), \
                    mock.patch.object(circleci.urllib.request, "urlopen", side_effect=outcomes) as urlopen:
                if ok:
                    self.assertEqual(circleci.fetch("u"), "ok")
                else:
                    with self.assertRaises(OSError):
                        circleci.fetch("u")
                self.assertEqual(urlopen.call_count, calls)

    def test_gate_passed_requires_a_successful_required_gate_from_the_circleci_app(self):
        bot = {"login": "circleci-app[bot]"}
        cases = [
            ([{"context": "ci/circleci: rsl", "state": "success", "creator": bot}], True),
            ([{"context": "ci/circleci: rsl", "state": "failure", "creator": bot}], False),
            # Newest first: a later red overrides an earlier green, and vice versa.
            ([{"context": "ci/circleci: rsl", "state": "failure", "creator": bot},
              {"context": "ci/circleci: rsl", "state": "success", "creator": bot}], False),
            ([{"context": "ci/circleci: rsl", "state": "success", "creator": bot},
              {"context": "ci/circleci: rsl", "state": "failure", "creator": bot}], True),
            ([{"context": "ci/circleci: rsl", "state": "failure", "creator": {"login": "someone"}},
              {"context": "ci/circleci: rsl", "state": "success", "creator": bot}], True),
            ([{"context": "ci/circleci: rsl-scoped", "state": "success", "creator": bot}], False),
            ([{"context": "ci/circleci: rsl", "state": "success", "creator": {"login": "someone"}}], False),
            ([{"context": "ci/circleci: rsl", "state": "success", "creator": None}], False),
            ([], False),
        ]
        for statuses, expected in cases:
            with self.subTest(statuses=statuses), mock.patch.object(circleci, "github", return_value=statuses):
                self.assertIs(circleci.gate_passed("a" * 40), expected)
        with mock.patch.object(circleci, "github", side_effect=OSError("rate limited")):
            self.assertFalse(circleci.gate_passed("a" * 40))

    def test_first_parent_reads_the_checkout(self):
        with tempfile.TemporaryDirectory(prefix="rsl-ci-parent-test-") as directory:
            def git(*args):
                # Plumbing only: no hooks, signing, or user configuration take part.
                identity = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t",
                            "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"}
                return subprocess.run(["git", *args], cwd=directory, check=True, stdout=subprocess.PIPE,
                                      env={**os.environ, **identity}).stdout.decode().strip()

            git("init", "--quiet")
            tree = git("write-tree")
            root = git("commit-tree", tree, "-m", "root")
            git("update-ref", "HEAD", root)
            self.assertEqual(circleci.first_parent(directory), "")
            git("update-ref", "HEAD", git("commit-tree", tree, "-p", root, "-m", "child"))
            self.assertEqual(circleci.first_parent(directory), root)
            self.assertTrue(circleci.descends(directory, root))
            git("update-ref", "HEAD", git("commit-tree", tree, "-m", "rewritten"))
            self.assertFalse(circleci.descends(directory, root))

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
    def test_a_manual_base_cannot_report_the_required_gate(self):
        plan = test_plan.planned(["protocols/session/socks/src/lib.rs"])
        jobs = cont(plan, BNB_GATE, {"CIRCLE_BRANCH": "feat/x", "CI_INPUT_BASE": "c" * 40})["jobs"]
        self.assertIn("rsl-scoped", jobs)
        self.assertNotIn("rsl", jobs)

    def test_prose_schedules_only_policy_and_the_gate(self):
        config = cont(test_plan.planned(["README.md"]), BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        self.assertEqual(set(config["jobs"]), {"policy", "commitlint", "rsl"})
        self.assertEqual(json.loads(config["jobs"]["rsl"]["environment"]["CI_NEEDS"]),
                         {"checks": "skipped", "fuzz": "skipped"})

    def test_only_selected_profiles_and_fuzz_targets_are_scheduled_concurrently(self):
        plan = test_plan.planned(["protocols/session/socks/src/lib.rs"])
        config = cont(plan, BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        selected = {f"checks-{circleci.bucket_of(profile)}" for profile in plan["profiles"] if profile != "workspace"}
        selected |= {f"checks-workspace-{part}" for part in ("lint", "test", "extra")} | {"commitlint"}
        self.assertEqual(set(config["jobs"]) - {"policy", "rsl"}, selected)
        workflow = {name: spec for entry in config["workflows"]["rsl"]["jobs"] for name, spec in entry.items()}
        for name in selected - {"commitlint"}:
            self.assertEqual(workflow[name], {"requires": ["policy"]})
        self.assertEqual(workflow["commitlint"], {})
        self.assertEqual(set(workflow["rsl"]["requires"]), selected | {"policy"})

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

    def test_only_full_trunk_plans_save_caches(self):
        scoped = test_plan.planned(["protocols/session/socks/src/lib.rs"])
        full = test_plan.planned(full=True)
        for plan, branch, trigger, saves in ((full, "main", "scheduled_pipeline", True),
                                             (full, "main", "webhook", False),  # full-reason push
                                             # A full push that changes a key saves it mid-week.
                                             ({**full, "paths": ["Cargo.lock"]}, "main", "webhook", True),
                                             ({**scoped, "paths": ["Cargo.lock"]}, "main", "webhook", False),
                                             (scoped, "main", "scheduled_pipeline", False),
                                             (full, "feat/x", "scheduled_pipeline", False)):
            config = cont(plan, BNB_GATE, {"CIRCLE_BRANCH": branch, "CI_TRIGGER_SOURCE": trigger})
            steps = [step for step in config["jobs"]["checks-workspace-test"]["steps"] if isinstance(step, dict)]
            self.assertEqual(any("save_cache" in step for step in steps), saves)
            # A saving job starts from its exact key only, so the saved key holds a clean build.
            keys = next(step["restore_cache"]["keys"] for step in steps if "restore_cache" in step)
            self.assertEqual(len(keys), 1 if saves else 3)

    def test_saved_caches_are_pruned_and_expire_before_the_next_weekly_save(self):
        weekly = {"CIRCLE_BRANCH": "main", "CI_TRIGGER_SOURCE": "scheduled_pipeline"}
        config = cont(test_plan.planned(full=True), BNB_GATE, weekly)
        prune = next(step["run"]["command"] for step in config["jobs"]["checks-workspace-test"]["steps"]
                     if isinstance(step, dict) and step.get("run", {}).get("name", "").startswith("Prune"))
        self.assertEqual(prune.split()[2:4], ["target", "codecs/bitsandbytes/bnb/nostd-check/target"])
        for name, job in config["jobs"].items():
            steps = [step for step in job["steps"] if isinstance(step, dict)]
            saves = [step["save_cache"] for step in steps if "save_cache" in step]
            with self.subTest(name):
                self.assertEqual(job.get("retention"), {"caches": "6d"} if saves else None)
                if saves:
                    # The prune runs immediately before the save, over every cached target.
                    prune = steps[steps.index({"save_cache": saves[0]}) - 1]["run"]["command"]
                    self.assertTrue(prune.startswith("python3 scripts/ci/prune.py "))

    def test_cache_keys_name_toolchain_image_and_lockfile(self):
        config = cont(test_plan.planned(full=True), BNB_GATE,
                      {"CIRCLE_BRANCH": "main", "CI_TRIGGER_SOURCE": "scheduled_pipeline"})
        steps = [step for step in config["jobs"]["checks-workspace-test"]["steps"] if isinstance(step, dict)]
        key = next(step["save_cache"]["key"] for step in steps if "save_cache" in step)
        self.assertIn(circleci.STABLE, key)
        self.assertIn(circleci.IMAGE.rsplit(":", 1)[1][:12], key)
        self.assertTrue(key.endswith(f'{{{{ checksum "Cargo.lock" }}}}-{circleci.cache_week()}'))
        self.assertNotIn("epoch", key)

    def test_a_cache_week_starts_at_the_weekly_full_schedule(self):
        from datetime import datetime, timezone
        for when, week in (("2026-11-15T09:16", "2026-11-08"), ("2026-11-15T09:17", "2026-11-15"),
                           ("2026-11-18T12:00", "2026-11-15"), ("2026-11-21T23:59", "2026-11-15")):
            with self.subTest(when):
                now = datetime.fromisoformat(when).replace(tzinfo=timezone.utc)
                self.assertEqual(circleci.cache_week(now), week)

    def test_short_profiles_share_jobs_of_one_toolchain(self):
        jobs = cont(test_plan.planned(full=True), BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})["jobs"]
        checks = {name for name in jobs if name.startswith("checks-")}
        self.assertLessEqual(len(checks), 8)
        self.assertEqual(jobs["checks-crates"]["environment"]["PROFILES"].split()[0], "deny")
        listed = [profile for members in circleci.BUCKETS.values() for profile in members]
        self.assertEqual(len(listed), len(set(listed)))
        self.assertTrue(set(listed) <= PROFILES)
        for members in circleci.BUCKETS.values():
            self.assertEqual(len({circleci.toolchain_of(p, BNB_GATE) for p in members if p not in circleci.NO_RUST}), 1)

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
                    for job, result in json.loads(config["jobs"]["rsl"]["environment"]["CI_NEEDS"]).items()}}
        report = coverage_report(handed_on(plan), needs, plan, event="pull_request", run_id="w", attempt=1)
        self.assertEqual(report["plan"], plan)


class ReleasePathTests(unittest.TestCase):
    """Only a reviewed trunk push reaches a secret, and evidence that can authorise publication
    never comes from a cache."""

    def workflow(self, config):
        return {name: spec for entry in config["workflows"]["rsl"]["jobs"] for name, spec in entry.items()}

    def test_release_qualifying_pipelines_restore_and_save_no_cache(self):
        for branch in ("main", "feat/x"):
            config = cont(test_plan.planned(release=True), BNB_GATE, {"CIRCLE_BRANCH": branch})
            for name, job in config["jobs"].items():
                with self.subTest(branch=branch, job=name):
                    kinds = {key for step in job["steps"] if isinstance(step, dict) for key in step}
                    self.assertFalse(kinds & {"restore_cache", "save_cache"})

    def test_auto_routes_to_the_runner_only_while_its_heartbeat_is_fresh(self):
        now = 1_800_000_000
        for seen, expected in ((None, "medium"), ("", "medium"), ("junk", "medium"),
                               (str(now - 30), circleci.RUNNER_CLASS), (str(now + 30), circleci.RUNNER_CLASS),
                               (str(now - circleci.RUNNER_FRESH - 1), "medium"), (str(now + 120), "medium")):
            env = {} if seen is None else {"CI_RUNNER_SEEN": seen}
            for chosen in (None, "auto"):
                with self.subTest(seen=seen, chosen=chosen):
                    self.assertEqual(circleci.routed_class({**env, **({"CI_INPUT_CLASS": chosen} if chosen else {})}, now),
                                     expected)
        # An explicit class wins over the heartbeat, in both directions.
        fresh = {"CI_RUNNER_SEEN": str(now)}
        self.assertEqual(circleci.routed_class({**fresh, "CI_INPUT_CLASS": "medium"}, now), "medium")
        self.assertEqual(circleci.routed_class({"CI_INPUT_CLASS": "ns/runner"}, now), "ns/runner")
        self.assertEqual(circleci.routed_class(fresh, now, cloud=True), "medium")
        self.assertEqual(circleci.routed_class({"CI_RUNNER_SEEN": "\u00b2"}, now), "medium")  # No ValueError.

    def test_cache_saving_and_release_runs_stay_on_the_cloud_with_a_fresh_runner(self):
        fresh = {"CI_RUNNER_SEEN": str(int(time.time()))}
        routine = cont(test_plan.planned(full=True), BNB_GATE, {"CIRCLE_BRANCH": "feat/x", **fresh})
        self.assertEqual(routine["jobs"]["policy"]["resource_class"], circleci.RUNNER_CLASS)
        weekly = cont(test_plan.planned(full=True), BNB_GATE,
                      {"CIRCLE_BRANCH": "main", "CI_TRIGGER_SOURCE": "scheduled_pipeline", **fresh})
        release = test_plan.planned(release=True)
        release["release_candidate"] = True
        qualifying = cont(release, BNB_GATE, {"CIRCLE_BRANCH": "main", **fresh})
        for config in (weekly, qualifying):
            self.assertTrue(all(job.get("resource_class") in ("medium", "large", None)
                                for job in config["jobs"].values()))

    def test_a_self_hosted_class_keeps_build_state_on_the_runner_not_in_circleci_caches(self):
        plan = test_plan.planned(full=True)
        for branch, side in (("main", "trunk"), ("feat/x", "branch")):
            config = cont(plan, BNB_GATE, {"CIRCLE_BRANCH": branch, "CI_INPUT_CLASS": "ns/runner"})
            checks = {name: job for name, job in config["jobs"].items() if name.startswith("checks-")}
            self.assertTrue(checks)
            for name, job in checks.items():
                with self.subTest(branch=branch, job=name):
                    kinds = {key for step in job["steps"] if isinstance(step, dict) for key in step}
                    self.assertFalse(kinds & {"restore_cache", "save_cache"})  # Billed egress.
                    self.assertEqual(job["resource_class"], "ns/runner")
                    links = [c for c in run_commands(job) if "ln -sfn" in c]
                    if links:
                        self.assertIn(f"/ci-cache/target/{side}/{name}-", links[0])
                        self.assertIn("ln -sfn /ci-cache/crates ~/.cargo/registry/cache", links[0])
            self.assertTrue(any(any("ln -sfn" in c for c in run_commands(job)) for job in checks.values()))
            # Policy and the gate run on the runner; checks start without waiting for policy.
            self.assertEqual(config["jobs"]["policy"]["resource_class"], "ns/runner")
            self.assertEqual(config["jobs"][circleci.GATE]["resource_class"], "ns/runner")
            workflow = self.workflow(config)
            self.assertFalse(any(workflow[name] for name in checks))
            self.assertIn("policy", workflow[circleci.GATE]["requires"])
        # A cloud class is unchanged: CircleCI caches, no runner links, checks after policy.
        cloud = cont(plan, BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        self.assertFalse(any("ln -sfn" in c for job in cloud["jobs"].values() for c in run_commands(job)))
        self.assertEqual(cloud["jobs"]["policy"]["resource_class"], "medium")
        self.assertEqual(cloud["jobs"][circleci.GATE]["resource_class"], "medium")
        self.assertTrue(all(self.workflow(cloud)[name] == {"requires": ["policy"]}
                            for name in cloud["jobs"] if name.startswith("checks-")))

    def test_release_jobs_exist_only_for_a_trunk_push(self):
        plan = test_plan.planned(release=True)
        plan["release_candidate"] = True
        for event in ("pull_request", "workflow_dispatch"):
            with self.subTest(event=event):
                config = continuation(plan, BNB_GATE, {"CIRCLE_BRANCH": "main"}, event)
                self.assertFalse({"release", "release-pr"} & config["jobs"].keys())
        workflow = self.workflow(continuation(plan, BNB_GATE, {"CIRCLE_BRANCH": "main"}, "push"))
        self.assertEqual(workflow["release"], {"requires": ["rsl"], "context": ["github", "crates"],
                                               "serial-group": circleci.RELEASE_GROUP})
        self.assertEqual(workflow["release-pr"], {"requires": ["release"], "context": ["github"],
                                                  "serial-group": circleci.RELEASE_GROUP})
        self.assertEqual(circleci.RELEASE_GROUP, "<< pipeline.project.slug >>/release")

    def test_a_trunk_push_without_a_candidate_only_refreshes_the_release_pr(self):
        config = cont(test_plan.planned(["README.md"]), BNB_GATE, {"CIRCLE_BRANCH": "main"})
        workflow = self.workflow(config)
        self.assertNotIn("release", config["jobs"])
        self.assertEqual(workflow["release-pr"]["requires"], ["rsl"])
        self.assertNotIn("crates", workflow["release-pr"]["context"])

    def test_release_jobs_check_the_trunk_before_any_token_is_used(self):
        plan = test_plan.planned(release=True)
        plan["release_candidate"] = True
        jobs = continuation(plan, BNB_GATE, {"CIRCLE_BRANCH": "main"}, "push")["jobs"]
        for name in ("release", "release-pr"):
            with self.subTest(job=name):
                names = [step["run"]["name"] for step in jobs[name]["steps"] if isinstance(step, dict) and "run" in step]
                commands = run_commands(jobs[name])
                current = names.index("Stop unless the trunk still points at this commit")
                self.assertEqual(current, 0)
                self.assertTrue(all("RELEASE_PLZ_TOKEN" not in command for command in commands[:current + 1]))
                self.assertFalse(any("x-access-token:$" in command or "@github.com" in command for command in commands))
        names = [step["run"]["name"] for step in jobs["release"]["steps"] if isinstance(step, dict) and "run" in step]
        self.assertLess(names.index("Stop unless this is the exact merged release PR"),
                        names.index("Require complete exact-run release and downstream qualification"))
        self.assertLess(names.index("Require complete exact-run release and downstream qualification"),
                        names.index("release-plz release"))

    def test_release_pr_syncs_detached_lockfiles_after_release_plz(self):
        job = continuation(test_plan.planned(["README.md"]), BNB_GATE, {"CIRCLE_BRANCH": "main"}, "push")["jobs"]["release-pr"]
        commands = run_commands(job)
        self.assertEqual(commands[-2:], ['release-plz release-pr --git-token "$RELEASE_PLZ_TOKEN"',
                                         "python3 scripts/ci/circleci.py sync-release"])

    def test_sync_release_commits_through_the_api_on_the_fetched_tip_only(self):
        git = lambda *args, cwd: subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True,
                                                text=True).stdout.strip()
        # The throwaway repositories ignore the host's global and system git configuration.
        isolated = mock.patch.dict(os.environ, {"GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1",
                                                "RELEASE_PLZ_TOKEN": "token"})
        with isolated, tempfile.TemporaryDirectory(prefix="rsl-sync-release-") as directory:
            directory = Path(directory)
            origin, clone = directory / "origin.git", directory / "clone"
            git("init", "--bare", "--initial-branch=main", str(origin), cwd=directory)
            git("clone", str(origin), str(clone), cwd=directory)
            (clone / "a").mkdir()
            (clone / "a" / "Cargo.lock").write_text("old\n")
            git("add", ".", cwd=clone)
            git("-c", "user.name=t", "-c", "user.email=t@example.invalid", "commit", "-m", "chore: release", cwd=clone)
            git("push", "origin", "HEAD:refs/heads/main", "HEAD:refs/heads/release-plz-x", cwd=clone)
            head = git("rev-parse", "HEAD", cwd=clone)
            pr = {"number": 1, "head": {"ref": "release-plz-x", "sha": "stale",
                                        "repo": {"full_name": circleci.REPOSITORY}},
                  "base": {"ref": circleci.TRUNK, "repo": {"full_name": circleci.REPOSITORY}}}

            def rewrite(tree):
                (Path(tree) / "a" / "Cargo.lock").write_text("new\n")
                return ["a/Cargo.lock"]

            cwd = os.getcwd()
            try:
                os.chdir(clone)
                for sync, calls in ((rewrite, 1), (lambda tree: [], 0)):
                    with (self.subTest(changes=calls),
                          mock.patch.object(circleci, "github", return_value=[pr]),
                          mock.patch.object(circleci, "release_policy", return_value="release-plz-"),
                          mock.patch.object(circleci.release_sync, "sync", side_effect=sync),
                          mock.patch.object(circleci, "graphql",
                                            return_value={"createCommitOnBranch": {"commit": {"oid": "x"}}}) as api):
                        circleci.sync_release(clone, directory / f"ci{calls}")
                        self.assertEqual(api.call_count, calls)
                        if calls:
                            query, variables, token = api.call_args.args
                            self.assertEqual((query, token), (circleci.COMMIT_ON_BRANCH, "token"))
                            self.assertEqual(variables, {"input": {
                                "branch": {"repositoryNameWithOwner": circleci.REPOSITORY,
                                           "branchName": "release-plz-x"},
                                "message": {"headline": "chore: sync detached lockfiles to the released versions"},
                                "fileChanges": {"additions": [{"path": "a/Cargo.lock", "contents": "bmV3Cg=="}]},
                                # The tip Git fetched, never the API's possibly stale head SHA.
                                "expectedHeadOid": head,
                            }})
            finally:
                os.chdir(cwd)

    def test_graphql_errors_fail_the_job(self):
        response = mock.MagicMock()
        response.__enter__.return_value.read.return_value = json.dumps(
            {"data": None, "errors": [{"message": "Expected branch to point to x"}]}).encode()
        with mock.patch.object(circleci.urllib.request, "urlopen", return_value=response), \
                self.assertRaisesRegex(RuntimeError, "Expected branch"):
            circleci.graphql("q", {}, "token")

    def test_decisions_halt_but_failures_fail(self):
        script = circleci.skip_unless("x", "decide")["run"]["command"]
        for status, outcome in ((0, "continue"), (circleci.SKIP, "halt"), (1, "fail"), (2, "fail")):
            with self.subTest(status=status):
                shell = script.replace("decide", f"(exit {status})").replace("circleci-agent step halt", "echo halt")
                result = subprocess.run(["bash", "-eo", "pipefail", "-c", shell + "\necho continue"],
                                        capture_output=True, text=True)
                if outcome == "fail":
                    self.assertEqual(result.returncode, status)
                else:
                    self.assertEqual((result.returncode, result.stdout.split()[0]), (0, outcome))

    def test_the_image_publishes_only_from_a_tested_trunk_push(self):
        plan = test_plan.planned([".circleci/image/Dockerfile"])
        self.assertNotIn("image", cont(plan, BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})["jobs"])
        workflow = self.workflow(cont(plan, BNB_GATE, {"CIRCLE_BRANCH": "main"}))
        self.assertEqual(workflow["image"], {"requires": ["rsl"], "context": ["images"]})

    def test_only_release_and_image_jobs_hold_contexts(self):
        plan = test_plan.planned(release=True)
        plan["release_candidate"] = True
        workflow = self.workflow(continuation(plan, BNB_GATE, {"CIRCLE_BRANCH": "main"}, "push"))
        holders = {name for name, spec in workflow.items() if spec and spec.get("context")}
        self.assertEqual(holders, {"release", "release-pr"})


class ImageTests(unittest.TestCase):
    """The image, its users and the selector's tool pins must not drift apart."""

    def setUp(self):
        self.dockerfile = (ROOT / ".circleci/image/Dockerfile").read_text()

    def arg(self, name):
        return re.search(rf"^ARG {name}=(\S+)$", self.dockerfile, re.M).group(1)

    def test_the_generator_and_setup_config_pin_their_images_by_digest(self):
        self.assertRegex(circleci.IMAGE, r"^ghcr\.io/rawsocketlabs/rsl-ci@sha256:[0-9a-f]{64}$")
        self.assertRegex((ROOT / ".circleci/config.yml").read_text(),
                         r"\n      - image: ghcr\.io/rawsocketlabs/rsl-ci-plan@sha256:[0-9a-f]{64}\n")

    def test_the_dockerfile_holds_the_toolchains_jobs_select_or_the_pending_stable(self):
        bnb = tomllib.loads((ROOT / ".github/ci/bnb.toml").read_text())
        # The Dockerfile at HEAD is the pinned image's, or holds a pending stable mid-bump.
        self.assertEqual(self.arg("STABLE"), circleci.NEXT_STABLE or circleci.STABLE)
        self.assertNotEqual(circleci.NEXT_STABLE, circleci.STABLE,
                            "clear NEXT_STABLE once STABLE reaches it")
        self.assertEqual(self.arg("MSRV"), circleci.MSRV)
        self.assertEqual(self.arg("NIGHTLY"), bnb["public_api_toolchain"])
        self.assertEqual(self.arg("PUBLIC_API"), bnb["public_api_version"])
        self.assertEqual(self.arg("SEMVER"), bnb["semver_version"])

    def test_a_cache_refresh_follows_the_job_image_pin_not_every_generator_edit(self):
        plan = {"base": "a" * 40, "paths": [circleci.GENERATOR]}
        with mock.patch.object(circleci, "image_pin_at", return_value=circleci.IMAGE):
            self.assertFalse(circleci.cache_inputs_changed(plan))
            self.assertFalse(circleci.cache_inputs_changed({**plan, "paths": [".circleci/config.yml"]}))
            self.assertTrue(circleci.cache_inputs_changed({**plan, "paths": ["Cargo.lock"]}))
        for before in ("ghcr.io/rawsocketlabs/rsl-ci@sha256:" + "0" * 64, None):  # Moved, or unreadable.
            with self.subTest(before=before), mock.patch.object(circleci, "image_pin_at", return_value=before):
                self.assertTrue(circleci.cache_inputs_changed(plan))
        self.assertEqual(circleci.pinned_image((ROOT / circleci.GENERATOR).read_text()), circleci.IMAGE)

    def test_the_plan_image_carries_the_job_image_cargo(self):
        # The gate recomputes the plan on rsl-ci, so the plan image's cargo must read metadata alike.
        plan = (ROOT / ".circleci/image/plan.Dockerfile").read_text()
        self.assertEqual(re.search(r"^ARG CARGO=(\S+)$", plan, re.M).group(1), self.arg("STABLE"))

    def test_a_checkout_pins_the_stable_toolchain_jobs_run(self):
        pin = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]
        self.assertEqual(pin["channel"], circleci.STABLE)
        for installed in (*pin["components"], *pin["targets"]):  # No job installs at run time.
            self.assertIn(installed, self.dockerfile)

    def test_msrv_matches_the_workspace_floor(self):
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertEqual(manifest["workspace"]["package"]["rust-version"] + ".0", circleci.MSRV)


if __name__ == "__main__":
    unittest.main()
