# GDSuite in the Godot Editor

## Status

Proposed. Phase 1 of the editor work in
[docs-and-editor-overhaul](docs-and-editor-overhaul.md), section 2, revised
for Godot 4.6 and 4.7 and for gdsuite 0.5 (tools built in). The GDExtension
and the Asset Store come after, see [bundled-tools](bundled-tools.md).

## What the editor offers now

Godot's editor changed a lot in 4.6 and 4.7, and it changes what an addon
should do:

- **Every panel is a dock.** In 4.6 the bottom panel joined the docking
  system: docks move between the sides and the bottom, and most can float in
  their own window, on another monitor. Plugins get the same through
  `EditorDock` (experimental) and `EditorPlugin.add_dock()`, with
  `available_layouts` saying where the dock fits. The old
  `add_control_to_bottom_panel()` still works, but is deprecated.
- **Rebindable shortcuts.** `EditorSettings.add_shortcut()` (4.6) puts a
  plugin's shortcuts in the editor's Shortcuts settings, where users rebind
  them.
- **Clickable output.** A file path in an error or warning in the Output panel
  opens the script at that line (4.6).
- **A quieter look.** The default theme is "Modern" (4.6): grayscale, little
  decoration. An addon should use the editor's theme and icons, not its own
  colors.
- **The Asset Store** replaces the Asset Library in the editor (4.7).

What tools in the same space do:

- **GDQuest's GDScript Formatter** adds a Format menu to the script editor,
  commands in the command palette, a rebindable shortcut, and Format On Save
  and Lint On Save in Editor Settings. Lint results show as gutter icons and
  line highlights in the script editor, and as a list in Output. It downloads
  its formatter binary into Godot's cache once per computer.
- **GdUnit4** has a test explorer dock: a tree of discovered tests, run from
  the FileSystem dock, the script editor or the tree, failures shown on
  selection, rebindable run, debug and stop shortcuts, and "Create test case"
  on a function in the script editor.

Sources: [Godot 4.6 release](https://godotengine.org/releases/4.6/),
[Godot 4.6: what changes for you (GDQuest)](https://www.gdquest.com/library/godot_4_6_workflow_changes/),
[EditorDock](https://docs.godotengine.org/en/stable/classes/class_editordock.html),
[Godot 4.7 release](https://godotengine.org/releases/4.7/),
[GDScript Formatter add-on](https://gdquest.com/library/gdscript_formatter_godot_addon),
[GdUnit4](https://godot-gdunit-labs.github.io/gdUnit4/latest/first_steps/run-tests).

## Principles

1. **Where you already look.** Problems show on the line in the script
   editor first; a list is for the rest of the project.
2. **Fast things run by themselves, slow things when asked.** Formatting and
   linting the saved script take milliseconds: they can run on save. Type
   checking the project and running tests start Godot: they run in the
   background or on request. Code health is a review, not a per-save check.
3. **Green in the editor means green in CI.** The editor runs the same
   `gdsuite` commands with the same `gdsuite.toml`, never a different
   configuration.
4. **Your preferences are yours; the project's rules are the team's.** What
   happens on save is a personal choice (Editor Settings); which rules apply
   and which steps a check runs belongs to the project (`gdsuite.toml`).
5. **Use the editor's own parts:** docks, its theme and icons, the command
   palette, rebindable shortcuts, context menus.

## Feature by feature

| | Where | Runs | Dedicated window? |
|---|---|---|---|
| **Formatting** | Script editor: "Format script" in a GDSuite menu, the command palette and a shortcut. No panel. | On Check, and on save, with Autofix on (the default) | No |
| **Lint** | Gutter icon and line tint on the script, message on hover; the Problems dock for the project | The saved script on save (**Check → On save**, on by default); the project on Check | No |
| **Type check** | Same Problems dock, its own filter | In the background a moment after you save; on Check | No |
| **Tests** | A Tests dock: tree, run, failures. A ▶ in the gutter of each `test_` function; "Run tests" on folders in the FileSystem dock | On request; on save with **Tests → On save** on | **Yes**: the dock floats well on a second monitor |
| **Code health** | Its findings in the Problems dock; its dependency graph as the dock's **Graph** view; the full report in the browser | On Check, and a moment after a save | No |
| **Check** | An icon in the editor's top bar, and the same **Check** in the Problems dock | On request | No |

### Formatting

- **Format script** formats the editor's text through
  `gdsuite fmt -- -` (stdin), then replaces the text as one undo step,
  keeping the caret and scroll position. It never rewrites a file behind an
  open, unsaved script.
- **Check** formats the project's scripts first, with **Autofix** on (the
  default), and so does the check on save, on the saved script: one setting
  for both, so what a save leaves is what Check would.
- A file that would change meaning is left alone, as on the command line; the
  editor says why in a toast, not a dialog.

### Lint and type check: the Problems dock

- One list for both, grouped by script, newest run first. Filters: errors or
  warnings, lint or types, this script or the whole project. A count on the
  dock's tab.
- Click a line to open the script at the line and column.
- Each finding has its rule as a link to its documentation page, and, when it
  can be fixed, a **Fix** button (`lint -- --fix` on that script). Check, with
  Autofix on, applies those fixes to the whole project first.
- Right-click a finding: **Turn off this rule for the project** (writes
  `gdsuite.toml`) or **Ignore here** (adds the suppression comment on the
  line).
- In the script editor: an icon in the gutter and a light tint on lines with
  findings, using the editor's error and warning colors; hover for the
  message.
- The dock lists **everything the type check reports**, including errors in
  scripts that are open, which Godot also shows in its own way. A problem
  never disappears from the list because its script was opened: the list is
  the project's state, whichever scripts are open. Beyond what Godot shows,
  it adds errors in scripts you haven't opened, missing resources and broken
  scene references.

### Tests: the Tests dock

Today's gdtest panel, moved into an `EditorDock` that can sit at the bottom,
on a side, or float:

- A tree of test scripts and tests with their status and time; run all, a
  folder, a script, one test; **Test failed**; **Test all**, which is the
  top bar's flask too and becomes Stop while tests run.
- A failure shows the message, expected and actual side by side, and the line;
  click to open it.
- In the script editor, a ▶ in the gutter of every `func test_` runs that test,
  and its last result shows as an icon there.
- **Run tests** in the FileSystem dock's context menu, on a folder or a
  script.
- **On save** (off by default, in the dock's ⚙): the dock runs the affected tests once
  per save of a script, scene or resource, as `gdtest --affected-by` lists
  them from the project's dependency graph. Saves during a run wait
  for it to end. (A first version kept gdtest's watch mode running; it
  started a full run when turned on and its runs could not show progress, so
  the dock now starts each run itself.)

### Code health: in the Problems dock

Code health is a source of problems like the others, so it has no dock of
its own: its findings are listed by script with the rest, under a **Health**
filter, critical ones as errors and the others as warnings. A finding a lint
rule also makes, on the same line, is listed once, as the lint problem.

The dock's **List | Graph** switch shows the dependency graph instead: one
box per script with its problem counts, users on the left and what they use
on the right, cycles in red. **Current** narrows it to the script being
edited, its users and what it uses. It comes from `gdsuite health --quick`,
which skips Git history and takes about a second. "Open the full health
report" runs `gdsuite health` and opens its HTML report.

### Check: the top bar

A **Check** icon in the editor's top bar, coloured by what the Problems dock
lists, does what the dock's **Check** button does: with **Autofix** on (the
default), format and apply the safe lint fixes to the scripts without unsaved
changes, then check format, lint, compile and health, as far as **A check
covers** says. Next to it, the flask does what the Tests dock's **Test all**
does.

## Settings

Two places, each for what it's good at. Each dock's ⚙ menu holds its
settings, with titles in the same shape: the Problems dock's Check (Autofix,
On save), A check covers (Format, Lint, Compile, Health), Show (Problems in
the script editor, Check in the top bar); the Tests dock's Tests (On save),
Show (Test all in the top bar). The toolbars have no toggles of their own.

**Your preferences: Editor Settings → GDSuite** (per computer, all projects):

| Setting | Default |
|---|---|
| Autofix, on Check and on save | On |
| Check on save | On (the saved script, then the project in the background) |
| Run the affected tests on save | Off |
| The top bar's Check and Test all | On |
| A check covers: Format, Lint, Compile, Health | On |
| Show problems in the gutter | On |
| `gdsuite` program | Found automatically |

Shortcuts (format script, run tests at cursor, rerun failed, check) are
registered with `EditorSettings.add_shortcut()`, so they show up and are
rebound in the editor's Shortcuts tab.

**The project's rules: `gdsuite.toml`**, which the terminal and CI read too.
The panel edits it for the common changes (turn a rule off, the steps of
Check, the test folder), through a `gdsuite` command that keeps the file's
comments, and has **Open gdsuite.toml** for the rest. A team that wants
everyone to run the affected tests on save says so in `gdsuite.toml`:

```toml
[editor]
test_on_save = true
```

which takes precedence over the personal setting, and the settings page shows
it ("set by this project").

## How it works

- **A GDScript addon, `addons/gdsuite/`,** kept in the gdsuite repository and
  built into the `gdsuite` program like gdtest's addons. `gdsuite init` and
  the first `gdsuite test` install it next to `addons/gdtest/` (the runtime
  tests need). It replaces `addons/gdtest_editor/`, which stays in gdtest for
  people who use gdtest alone; gdsuite removes the copy it installed, with a
  backup.
- **It only runs `gdsuite`,** in the background (`OS.create_process` and
  `OS.execute_with_pipe` on a thread), so the editor never freezes. It finds
  the program in this order: the Editor Setting, a link that `gdsuite`
  keeps in `.gdsuite/bin/` (like the test panel's `gdtest` today), then
  `PATH`. When none is found, the panel says how to install GDSuite.
- **Godot 4.5 and newer,** as the rest of GDSuite. Docks use `EditorDock` on
  4.6 and newer, and the bottom panel on 4.5; shortcuts fall back to a plain
  menu on 4.5.
- **The addon is GDScript that GDSuite checks:** formatted, linted and type
  checked by gdsuite in CI, with tests for its parsing code.

### What the command line needs

| Need | Change |
|---|---|
| Format the editor's text with the project's `[fmt]` settings | `gdtidy fmt -` reads defaults today: add `--stdin-filename <path>` so the project's settings and excludes apply |
| One problem format for lint and type check | `gdsuite typecheck --report json`, with the same fields as lint (path, line, column, end, severity, rule, message, fixable) |
| Lint one script fast | `gdsuite lint --report json <file>` exists |
| Test events while tests run | gdtest's JSON reporter exists; the panel already reads it |
| Health overview | `gdsuite health --report json` exists; check it has grades per group and hotspots with their numbers |
| Edit `gdsuite.toml` from the editor | `gdsuite config set|unset <key> <value>`, keeping comments and layout |
| Find gdsuite from the project | the `.gdsuite/bin/gdsuite` link |
| Team defaults for the editor | an `[editor]` section in `gdsuite.toml` |

## Phases

1. **Problems and formatting.** The addon, the top-bar Check, the Problems
   dock with gutter markers, Format script and format on save, the settings
   page, and the command-line changes they need.
2. **Tests.** The gdtest panel moved into the Tests dock, gutter ▶ and
   results, FileSystem "Run tests", rerun on save. `gdtest_editor` stops being
   installed.
3. **Health.** Its findings in the Problems dock, and the dependency graph.
4. **GDExtension.** The same addon calls gdsuite in-process instead of
   running it, which needs each tool to offer a library API that returns its
   results rather than printing them. Then the Asset Store.

Each phase ships on its own and is usable without the next.

## Decided

- **Godot's own warnings and errors:** the Problems dock shows everything the
  type check reports, including what Godot also shows for open scripts, so
  nothing disappears from the list when a script is opened.
- **Tests on save:** one run per save, started by the dock, of the tests
  `gdtest --affected-by` lists.
- **Code health:** a source of problems in the Problems dock, with its
  dependency graph as a view of that dock, not a dock or a main screen tab.
- **One Check, one Test all:** the top bar and the docks run the same
  thing, with the same icon. One Autofix for Check and for save. Settings
  live in each dock's ⚙ menu, not as toggles in its toolbar.
