// Enforce Conventional Commits so release-plz can derive each crate's SemVer bump.
// Extends the Angular/conventional preset, with two project tweaks:
//   * allow our extra `bench:` type (used for benchmark-only changes),
//   * don't cap body/footer line length (we keep detailed multi-line bodies and a
//     trailing `Co-Authored-By:` footer).
//
// ESM (`.mjs`) is required by wagoid/commitlint-github-action@v6.
//
// Reminder of the bump each type drives (via release-plz):
//   feat -> minor   fix -> patch   feat!/fix!/`BREAKING CHANGE:` -> major
//   build/chore/ci/docs/perf/refactor/revert/style/test/bench -> no bump.
export default {
  extends: ['@commitlint/config-conventional'],
  rules: {
    'type-enum': [
      2,
      'always',
      [
        'build',
        'chore',
        'ci',
        'docs',
        'feat',
        'fix',
        'perf',
        'refactor',
        'revert',
        'style',
        'test',
        'bench',
      ],
    ],
    'body-max-line-length': [0, 'always', Infinity],
    'footer-max-line-length': [0, 'always', Infinity],
  },
};
