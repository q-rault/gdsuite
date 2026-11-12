# GDSuite for Godot Users: Install, Editor, Docs

## Status

Draft, for discussion. Nothing here is implemented yet.

## The vision in one paragraph

A Godot developer installs **one thing, GDSuite**, from wherever they are
comfortable: the Godot Asset Store inside the editor, `cargo`, or a GitHub
release. From then on, formatting, linting, type checking, testing and code
health are buttons and panels in the Godot editor, and the same features are
`gdsuite <command>` in a terminal or CI. gdtidy, gdtypecheck, gdtest and
gdhealth are the engines under the hood: GDSuite installs and updates them,
and nobody has to know their names to use them. The docs are written for a
Godot user who may not be a professional developer: every feature starts
with a picture of what it does, then shows the editor way, then the terminal
way.

## Principles

1. **GDSuite is the product.** The tools are engines. The docs, the editor
   addon and the install paths all speak about GDSuite. Tool names appear in
   one "Under the hood" page and in footnotes, each time with "you don't need
   to install it separately".
2. **Editor first, terminal second, CI third.** The same feature, in that
   order, on every page.
3. **Show before explaining.** A before/after picture of formatting, a
   screenshot of a lint problem, a green/red test tree, a health map. Then the
   words.
4. **One source of truth.** The editor addon runs the `gdsuite` CLI; it never
   reimplements a tool or reads a tool binary directly. Settings live in
   `gdsuite.toml`, whatever surface changes them.
5. **No internal vocabulary in user docs.** "Suite-owned", "delegated",
   "product", "envelope", "contract", "depth command" go away.

---

## 1. Installing

> Building the tools into gdsuite ([bundled-tools](bundled-tools.md)) removes
> the per-project downloads below and makes plan B (GDExtension) the way to
> ship a complete Asset Store addon.

### The three channels, in this order

| Channel | For whom | What they get |
|---|---|---|
| **Godot Asset Store** (recommended) | Everyone | The GDSuite addon in `addons/gdsuite/`; first enable downloads the `gdsuite` program and the engines |
| **cargo** | People at ease with a terminal and Rust | `cargo install gdsuite` (or `cargo binstall gdsuite`), then `gdsuite init` in the project |
| **GitHub releases** | No Rust, CI images, offline machines | A `gdsuite` archive per platform, put on `PATH`, then `gdsuite init` |

Every channel ends in the same state: a `gdsuite` binary, `gdsuite.toml` in
the project, engines under `.gdsuite/`, and the editor addon. A CLI install
offers the addon (`gdsuite init` already installs the gdtest panel; it would
install `addons/gdsuite/` instead), and an Asset Store install gives the CLI
(the addon can put `gdsuite` on PATH on request, or show its path).

The Install page is one page with three tabs (Fumadocs `<Tabs>`), Asset Store
first, plus a "Check it worked" block that is the same for all three (the
addon's status line, or `gdsuite doctor`).

### The Asset Store constraint, and how to live with it

The store [guidelines](https://store.godotengine.org/guidelines/) reject
"standalone, compiled executables", but accept shared libraries listed in a
`.gdextension` file. So the addon **cannot ship the `gdsuite` binary**. Two
designs fit:

**A. Bootstrap download (recommended first).** The addon is pure GDScript.
On first enable, its panel shows "GDSuite needs to download its tools
(about N MB) from github.com/q-rault/gdsuite. [Download] [Use an existing
install…]". It downloads the release archive for the platform, checks its
SHA-256 against a manifest **shipped inside the addon** (so a given addon
version always installs the same, verified binary), unpacks it into
`.gdsuite/bin/` (already gitignored), and runs `gdsuite setup`. Same
mechanism for updates: a new addon version carries a new manifest.

- Pros: small addon, no new build target, reuses the release archives and the
  checksum machinery `gdsuite setup` already has.
- Risk: a moderator may dislike runtime downloads. The guidelines are silent
  on it. **Ask on the store's feedback channel before submitting**, describing
  the consent dialog and the checksum check. Keep "Use an existing install"
  so the addon also works fully offline with a cargo/GitHub install.

**B. GDExtension (later, if A is refused or for speed).** Build gdsuite's
core plus gdtidy as a `cdylib` with godot-rust, shipped per platform in the
addon. Formatting and linting then run in-process (instant format-on-save, no
process spawn). Tests and typecheck still need to start Godot, so the CLI
stays. Cost: four platform libraries per release, a Godot-version
compatibility matrix, a much larger engineering surface. Worth it only once
A works and format-on-save latency proves to matter.

### How to publish on the Asset Store (step by step)

Do this once the addon exists (section 2). From the
[submission docs](https://docs.godotengine.org/en/latest/community/asset_store/submitting_to_asset_store.html)
and the guidelines:

1. **Prepare the addon folder** as `addons/gdsuite/`, alone in the zip
   (no `.github`, `.gitignore`, `.DS_Store`, `Thumbs.db`). Put `LICENSE.md`
   (MIT/Apache text with year and copyright holder) *inside* `addons/gdsuite/`
   so it ships with the asset.
2. **Make the GDScript clean**: zero script warnings, official style guide.
   Run gdtidy on the addon in CI; the store asks for exactly what we sell.
3. **Test it on the minimum Godot version you will declare** (the lowest
   version gdtest's harness supports in CI) and on the latest stable.
4. **Create the listing**: sign in at store.godotengine.org → Upload Asset →
   publisher name and slug, asset name "GDSuite", type Addon.
5. **Settings tab**: summary and description in full English sentences (not
   a link to the repo), tags such as `testing`, `linter`, `formatter`,
   `tools` (not "gdsuite"), license matching the file, source code link, and
   the **AI usage disclosure**, which is mandatory when AI was used to build
   the asset.
6. **Media tab**: a clear 16:9 thumbnail readable at small size (logo plus
   one word, no screenshot collage), 3–5 screenshots (test tree, Problems
   list, format before/after, health view), optionally a 60-second video.
7. **Versions tab**: upload the zip, version name matching gdsuite's version,
   changelog, minimum Godot version.
8. **Submit for review**; a rejection comes with a reason, fix and resubmit.
9. **Automate the zip**: the gdsuite release workflow builds
   `gdsuite-addon-<version>.zip` as a release asset; uploading it to the store
   stays a manual step (no documented upload API).

---

## 2. GDSuite in the Godot editor

### Today

gdtest has an editor plugin (`addons/gdtest_editor/`): a bottom panel that
lists tests, runs them, and opens failures. `gdsuite setup` copies it in. It
finds the `gdtest` binary by guessing paths (`target/release/…`, `PATH`),
which fails for most users of gdsuite, whose gdtest lives in
`.gdsuite/tools/`.

### Target: one addon, `addons/gdsuite/`, one panel

```
┌ GDSuite ───────────────────────────────────────────────────────────────┐
│ [▶ Check]  ✔ Format  ✖ Lint (3)  ✔ Types  ● Tests 41/42  ◐ Health B   │
├─────────┬──────────────────────────────────────────────────────────────┤
│ Problems│ ✖ player.gd:42  unused-variable  "speed" is never used       │
│ Tests   │ ✖ enemy.gd:10   max-line-length  line is 132 characters      │
│ Health  │ ⚠ ui/hud.gd:88  Type error: "Label" has no "set_txt"         │
│ Setup   │                                                              │
└─────────┴──────────────────────────────────────────────────────────────┘
```

A status strip on top (one chip per feature, click to open its tab), and
four tabs. Every line that points at code opens the script at that line.

### Feature by feature

| Feature | In the editor | When it runs | Notes |
|---|---|---|---|
| **Format** | "Format this script" (script editor menu + shortcut), "Format project", **format on save** (opt-in toggle) | On demand; on save if enabled | Format the editor buffer through stdin (`gdtidy fmt -` exists, via `OS.execute_with_pipe`), replace the text, keep caret and undo history. Never rewrite a file behind an open buffer. |
| **Lint** | Problems tab; optional gutter marker on the line | On save, for the saved script; whole project on "Check" | `gdsuite lint --report json <file>`. "Fix" button for fixable rules. Each rule links to its doc page. |
| **Type check** | Same Problems tab, "Types" filter | Whole project on "Check" | Godot already shows errors of *open* scripts; the value here is project-wide: broken references in files you didn't open. Say so in the UI. |
| **Tests** | Today's gdtest panel, moved into the Tests tab; "Run tests in this script" and "Run test at cursor" | On demand; optional "rerun failed on save" | Keep the incremental JSON event stream gdtest already emits. Find the binary through gdsuite, not by guessing. |
| **Health** | Health tab: a grade per folder/group, the top hotspots (biggest, most complex, most coupled scripts), "Open dependency graph" | On demand only (slower, not a per-save thing) | Explain each number in a tooltip in plain words. The graph view can open gdhealth's HTML graph in the browser at first. |
| **Check** | The ▶ Check button: runs `[check].steps`, fills every chip | On demand | Same steps as `gdsuite check`, so "green in the editor" means "green in CI". |
| **Setup** | Setup tab: Godot found, engines installed and versions, "Update", "Open gdsuite.toml", a few common toggles (format on save, check steps) | First enable; on demand | First enable is the download/consent screen from section 1. Toggles write `gdsuite.toml`, never Project Settings, so the terminal and CI see the same thing. Editor-only preferences (format on save, shortcuts) are the exception: they belong to the user, so they go in Editor Settings. |

### What this needs from the CLI

The addon only ever calls `gdsuite`. To make that clean:

- `gdsuite doctor --output json` already gives paths and versions: the addon
  uses it to find everything.
- Per-file runs: `gdsuite fmt -- -` (stdin), `gdsuite lint --report json
  <files>`, `gdsuite typecheck --report json`, `gdsuite test --report json
  <path>` with the streamed events gdtest already writes.
- One problem format for lint and typecheck findings in JSON (file, line,
  column, severity, rule, message, fixable), so the Problems tab has a single
  parser.
- `gdsuite self-update` or "setup to the version this addon pins", for the
  addon's Update button.

### What happens to `addons/gdtest_editor`

It stays in the gdtest repo for people who use gdtest alone, but gdsuite
stops installing it: `gdsuite setup` installs `addons/gdsuite/` (whose Tests
tab is the same code) and removes an old `gdtest_editor` it installed, with a
backup like today's harness upgrade. `addons/gdtest/` (the runtime that test
scripts extend) is still installed: tests need it at run time.

---

## 3. The docs

### What is wrong today

Read page by page, the portal has four problems:

1. **It speaks to the maintainer, not the user.** "suite-owned entrypoint",
   "delegated depth commands", "product-owned", "envelope", "contract". A
   Godot user cannot tell from the home page what GDSuite does to *their
   game*.
2. **Every page wears the same template** ("What this page is for / When you
   should use this / What you need / Step-by-step / What success looks like /
   Common mistakes / Where to go next"). On 50-line pages the template is
   most of the page, and it repeats the same three commands each time.
3. **It is organized by command, not by need.** check/test/health each get a
   page; formatting and linting get none, although they are what a new user
   sees first. Writing tests, the real work, lives two clicks deep in
   imported gdtest pages.
4. **Stubs and leftovers.** Six recipe pages are 11 lines each; "Profiles"
   documents a rejected idea; the glossary defines internal jargon; the home
   page says "three products" (there are four engines); the troubleshooting
   output shows gdtidy 0.2.0; config pages link to GitHub READMEs instead of
   the portal; and the imported tool sections each have their own
   Installation page telling people to install the tool on its own.

### Page-by-page verdict

| Page | Makes sense? | Verdict |
|---|---|---|
| `index` (What is GDSuite?) | Abstract; product-boundary talk | **Rewrite** as Welcome: one sentence, one picture per feature, a "Start" button |
| `install` | Concrete but cargo-only | **Rewrite** with the three channels in tabs |
| `quickstart` | Lists commands, shows nothing | **Rewrite** as "Your first 10 minutes", editor first |
| `migrate` | Useful, mixes two cases | **Merge** with `recipes/existing-repo-onboarding` into "Add GDSuite to an existing project" |
| `workflow-check` | Good content (steps table) | **Keep content**, move into "Check everything at once" |
| `workflow-test` | Thin | **Merge** into the Testing feature page |
| `workflow-health` | Thin, abstract ("first-class workflow") | **Merge** into the Code health feature page |
| `ci` | Correct, dry | **Merge** with `recipes/ci-workflow` and `reports` into "Run it in CI", with a complete GitHub Actions file |
| `reports` | Explains envelopes vs bundles | **Merge** into "Run it in CI"; JSON fields go to Reference |
| `troubleshooting` | Concrete, useful | **Keep**, update outputs, add editor-side problems, rename "Troubleshooting & FAQ" |
| `config/index` | Clear | **Merge** into one "Configure GDSuite" page |
| `config/gdsuite-toml` | Clear, the best page | **Becomes** "Configure GDSuite" |
| `config/profiles` | Documents a rejected idea | **Delete** (keep the decision in an RFC) |
| `config/report-settings` | 20 lines | **Merge** into Configure |
| `config/product-config-generation` | Correct but maintainer-angled | **Merge**: "what init writes" into Install, "migrate" into existing-project page |
| `config/gdtest-compatibility`, `config/gdhealth-compatibility` | Mapping tables | **Merge** into one "Settings in tool files" section of Configure |
| `recipes/*` (6 pages) | 11-line stubs | **Delete**; their content folds into feature pages |
| `reference/cli` | Good | **Keep** |
| `reference/config-schema-reference` | Duplicates gdsuite-toml | **Merge** into a single "gdsuite.toml reference" |
| `reference/report-json-format` | Stub, no fields | **Fill** with real fields or delete |
| `reference/glossary` | Defines internal jargon | **Delete**; replace with plain-word definitions inside the feature pages |
| `gdtest/`, `gdtidy/`, `gdhealth/`, `gdtypecheck/` sections | Present the engines as products, each with its own install | **Restructure**: import only the deep pages (see below) under the feature they serve |

### The new structure

```
Get started
  Welcome                      what GDSuite does, one picture per feature
  Install                      Asset Store | cargo | GitHub release
  Your first 10 minutes        enable, Check, fix a problem, run a test

What GDSuite does              one page per feature, each: picture → editor → terminal → settings
  Formatting                   before/after; format on save
  Linting                      a problem in the panel; fixing; silencing a rule
  Type checking                what Godot misses in unopened scripts
  Testing                      what a test is, in a game; running tests
  Code health                  what the grades mean; when to look
  Check everything at once     the Check button = gdsuite check; choosing steps

Writing tests                  (imported from gdtest, renamed for the user)
  Your first test
  Assertions
  Testing scenes               scene runner
  Fakes and mocks              mocking
  Snapshots
  Coming from GUT / gdUnit4

Use it with your team
  Run it in CI                 complete workflow file; reports; artifacts
  Add GDSuite to an existing project   migrate; gdtoolkit; GUT/gdUnit4
  Configure GDSuite            gdsuite.toml, tool sections, tool files

Reference
  Commands                     reference/cli
  gdsuite.toml                 every key, including tool sections
  Lint rules                   imported from gdtidy (SARIF helpUri keeps working on gdtidy's site)
  Test API                     assertions, mocks, scene runner references
  Health metrics               imported from gdhealth
  Report formats               JSON

Troubleshooting & FAQ
Under the hood                 the four engines, why they exist, links to their own sites,
                               "you don't need to install them separately"
```

Order rationale: a newcomer reads top to bottom and is productive after the
first group; the feature group answers "what is this button?"; writing tests
gets its own group because it is the only part where the user writes code
against our API; team topics come after solo use; reference is looked up,
not read.

### Imported tool docs

Keep the tool sites (they serve people who use one engine alone, and
gdtidy's rule links in SARIF point there), but change what the portal
imports:

- Import an **allowlist** per tool (in `lib/docs/product-sources.ts`): the
  deep pages (test-writing guides, API references, lint rules, health
  metrics, formatting details). Do **not** import their index, why, install,
  quickstart and CLI pages: they compete with GDSuite's own.
- Imported pages show commands as gdsuite commands. The cheapest way: tool
  pages that show a command use a small `<Command tool="gdtest" args="…"/>`
  component that renders `gdtest …` on the tool site and `gdsuite test -- …`
  in the portal.
- Each tool site gets a banner: "Most Godot users want GDSuite, which
  includes this tool. →".

### Voice and visuals

- **Drop the page template.** Each feature page follows: *What it does (one
  sentence and a picture) → In the editor → In a terminal → Settings →
  Questions*. Pages are as long as their content, no longer.
- **Plain words.** Formatting: "tidies the layout of your scripts (spaces,
  indentation, blank lines) the same way every time; it never changes what
  the code does." Linting: "points at code that works but is likely a
  mistake or hard to read, like a variable you never use." Testing: "small
  scripts that play a piece of your game and check the result, so you know
  a change didn't break it." Code health: "shows which scripts have grown
  too big or too tangled, before they become painful to change."
- **Visual components to build once**: `<BeforeAfter>` (two code panes for
  formatting and lint fixes), editor screenshots in light and dark, short
  looping clips for the test panel and format on save, a health map image.
- **Remove jargon** listed in Principles; say "GDSuite runs gdtest" only on
  the Under the hood page.
- **Fix the factual drift**: four engines, current versions in sample
  outputs, portal links instead of GitHub README links.

---

## 4. Order of work

| Phase | Ships | Depends on |
|---|---|---|
| 1. Docs restructure | New structure, rewrites, deletions, import allowlist, Install page with cargo + GitHub tabs | Nothing; can start now |
| 2. Editor addon v1 | `addons/gdsuite/`: Setup tab with existing-install detection, Tests tab (ported gdtest panel), Problems tab (lint + typecheck), Check button, format this script | CLI per-file JSON runs, unified problem format |
| 3. Asset Store | Bootstrap download with shipped checksums, addon zip in releases, store listing; Install page gains the Asset Store tab (first) | Phase 2; moderators' answer on runtime download |
| 4. Editor addon v2 | Format on save, lint on save, gutter markers, Health tab, run test at cursor | Phase 2 |
| 5. Optional | GDExtension for in-process fmt/lint | Only if phase 3 is refused or latency demands it |

Write the docs for the editor in phase 1 only where the editor feature
exists; add each editor section as the feature ships, so the docs never
describe a button that isn't there.

## Open questions

1. Bootstrap download (A) vs GDExtension (B) for the Asset Store build.
2. Minimum Godot version the addon declares.
3. Whether tool sites stay fully indexed or point canonical URLs to the
   portal for the imported pages.
