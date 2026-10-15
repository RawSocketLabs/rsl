"""Local runner tests: it runs CircleCI's own generated jobs, never a local variant."""

import argparse
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import local
from test_circleci import BNB_GATE, cont
import test_plan


IDENTITY = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t",
            "GIT_COMMITTER_EMAIL": "t@t", "PATH": os.environ["PATH"], "HOME": os.environ.get("HOME", "/")}


def git(root, *args):
    subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, env=IDENTITY)


def git_out(root, *args):
    return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, text=True,
                          env=IDENTITY).stdout.strip()


class InputTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="rsl-ci-local-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        git(self.root, "init", "--quiet", "--initial-branch", "feat/x")
        # Plumbing: no hooks or signing configuration of the machine running the tests.
        tree = git_out(self.root, "hash-object", "-t", "tree", "-w", "/dev/null")
        commit = git_out(self.root, "commit-tree", tree, "-m", "a")
        git(self.root, "update-ref", "refs/heads/feat/x", commit)
        policy = mock.patch.object(local.circleci, "release_policy", return_value="release-plz-")
        policy.start()
        self.addCleanup(policy.stop)

    def inputs(self, mode="pre-push", base=None, full=False, product=None):
        return local.pipeline_inputs(argparse.Namespace(mode=mode, base=base, full=full, product=product), self.root)

    def test_default_base_is_the_merge_base_with_repo_guards_trunk(self):
        head = git_out(self.root, "rev-parse", "HEAD")
        git(self.root, "update-ref", "refs/remotes/origin/main", head)
        git(self.root, "config", "repoGuard.remote", "origin")
        git(self.root, "config", "repoGuard.trunk", "main")
        inputs = self.inputs()
        self.assertEqual((inputs["CI_INPUT_BASE"], inputs["CI_INPUT_FULL"]), (head, "false"))
        self.assertEqual(inputs["CIRCLE_BRANCH"], "feat/x")

    def test_full_breadth_keeps_the_base_release_inference_needs(self):
        head = git_out(self.root, "rev-parse", "HEAD")
        git(self.root, "update-ref", "refs/remotes/origin/main", head)
        git(self.root, "config", "repoGuard.remote", "origin")
        git(self.root, "config", "repoGuard.trunk", "main")
        inputs = self.inputs(mode="release", full=True)
        self.assertEqual((inputs["CI_INPUT_BASE"], inputs["CI_INPUT_FULL"], inputs["CI_INPUT_RELEASE"]),
                         (head, "true", "true"))

    def test_without_a_configured_trunk_coverage_is_full(self):
        self.assertEqual(self.inputs()["CI_INPUT_FULL"], "true")

    def test_release_mode_products_and_release_branches_qualify_releases(self):
        self.assertEqual(self.inputs(mode="release")["CI_INPUT_RELEASE"], "true")
        inputs = self.inputs(product=["bitsandbytes"])
        self.assertEqual((inputs["CI_INPUT_RELEASE"], inputs["CI_INPUT_PRODUCTS"]), ("true", "bitsandbytes"))
        self.assertEqual(self.inputs()["CI_INPUT_RELEASE"], "false")
        git(self.root, "checkout", "--quiet", "-b", "release-plz-2026")
        self.assertEqual(self.inputs()["CI_INPUT_RELEASE"], "true")


class ExecutionTests(unittest.TestCase):
    def test_jobs_run_the_generated_commands_unchanged(self):
        config = cont(test_plan.planned(full=True), BNB_GATE, {"CIRCLE_BRANCH": "feat/x"})
        for name, job in config["jobs"].items():
            script = local.job_script(name, job)
            for step in job["steps"]:
                if isinstance(step, dict) and "run" in step:
                    self.assertIn(step["run"]["command"], script)

    def test_release_jobs_never_run_and_the_gate_runs_last(self):
        config = cont(test_plan.planned(["README.md"]), BNB_GATE, {"CIRCLE_BRANCH": "main"})
        workflow = config["workflows"]["ci"]["jobs"]
        self.assertIn("release-pr", config["jobs"])
        stages = local.stages(workflow)
        self.assertEqual(stages[-1], ["CI"])
        self.assertIn("policy", stages[0])
        self.assertFalse(local.REMOTE_ONLY & {name for stage in stages for name in stage})

    def test_containers_run_the_given_image_unprivileged(self):
        command = local.podman("image@sha256:x", Path("/w"), Path("/t"), {"A": "1"}, "true", registry=Path("/r"))
        self.assertIn("--userns=keep-id:uid=1001,gid=1001", command)
        self.assertNotIn("--privileged", command)
        # Private labels would revoke the hardlinked objects other job clones share.
        self.assertFalse([arg for arg in command if arg.endswith(":Z")])
        self.assertEqual(command[-6:], ["image@sha256:x", "bash", "-eo", "pipefail", "-c", "true"])
        self.assertIn("A=1", command)


if __name__ == "__main__":
    unittest.main()
