const allowedTypes = [
  "feat",
  "fix",
  "refactor",
  "test",
  "docs",
  "chore",
  "perf",
];

const imperativeVerbs = new Set([
  "add",
  "fix",
  "refactor",
  "remove",
  "support",
  "inject",
  "prevent",
  "enable",
  "disable",
  "implement",
  "improve",
  "rename",
  "split",
  "merge",
  "simplify",
  "optimize",
  "configure",
  "document",
  "test",
  "clean",
  "align",
]);

const genericSubjects = new Set([
  "update",
  "update code",
  "update codes",
  "misc",
  "misc updates",
  "wip",
  "tmp",
]);

module.exports = {
  parserPreset: {
    parserOpts: {
      headerPattern: /^(\w+)(?:\(([a-z0-9-]+)\))?: (.+)$/i,
      headerCorrespondence: ["type", "scope", "subject"],
    },
  },
  plugins: [
    {
      rules: {
        "subject-not-generic": ({ subject }) => {
          const normalized = (subject || "").trim().toLowerCase();
          if (genericSubjects.has(normalized)) {
            return [
              false,
              'subject must be specific, avoid generic text like "update code"',
            ];
          }
          return [true];
        },
        "subject-imperative-verb": ({ subject }) => {
          const firstWord = (subject || "")
            .trim()
            .split(/\s+/)[0]
            ?.toLowerCase();
          if (!firstWord) {
            return [false, "subject must not be empty"];
          }
          if (!imperativeVerbs.has(firstWord)) {
            return [
              false,
              "subject should start with an imperative verb (e.g. add/fix/refactor/remove)",
            ];
          }
          return [true];
        },
      },
    },
  ],
  rules: {
    "type-enum": [2, "always", allowedTypes],
    // Scope is recommended; keep as warning to match "when possible".
    "scope-empty": [1, "never"],
    // Scope is free-form but should stay readable when present.
    "scope-case": [2, "always", ["kebab-case"]],
    "header-max-length": [2, "always", 72],
    "subject-empty": [2, "never"],
    "subject-full-stop": [2, "never", "."],
    "subject-case": [0],
    "subject-not-generic": [2, "always"],
    "subject-imperative-verb": [2, "always"],
  },
};
