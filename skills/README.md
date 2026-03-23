# Agent skills (skills.sh / `npx skills`)

Skills here follow the [Agent Skills](https://agentskills.io) shape: each skill is a directory with **`SKILL.md`** (YAML frontmatter + instructions). They install with the **[skills CLI](https://skills.sh)** (`npx skills`, open source: [vercel-labs/skills](https://github.com/vercel-labs/skills)).

## Install this skill

From a clone of this repo (project install — symlinks or copies into your agent’s skills dir):

```bash
npx skills add ./skills/sift-cli
# or pick agents explicitly, e.g.:
npx skills add ./skills/sift-cli -a cursor -a claude-code -y
```

After this repo is on GitHub, others can install from a path (branch name may be `main` or `master`):

```bash
npx skills add https://github.com/sift-rs/sift/tree/master/skills/sift-cli
```

Use your fork’s `org/repo` if it differs from the workspace `repository` URL in the root `Cargo.toml`.

List skills in the repo without installing:

```bash
npx skills add ./skills --list
```

## Skills

| Directory | Purpose |
|-----------|---------|
| [`sift-cli/`](sift-cli/) | Work on the **`sift`** CLI (`crates/cli`), flags, tests |
