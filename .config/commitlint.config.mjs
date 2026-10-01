/** @type {import('@commitlint/types').UserConfig} */
export default {
  extends: ['@commitlint/config-conventional'],
  // Ignore automated commits whose messages we don't control: the Blacksmith
  // runner-migration wizard (not Conventional Commits) and Dependabot, whose
  // conventional `build(deps):` subject is configured but whose generated body
  // carries URL lines longer than body-max-line-length allows.
  ignores: [
    (message) => /^Migrate .+ to Blacksmith\b/i.test(message),
    (message) => /^Signed-off-by: dependabot\[bot\]/m.test(message),
  ],
};
