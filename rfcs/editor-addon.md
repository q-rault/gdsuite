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
| **Formatting** | Script editor: "Format script" in a GDSuite menu, the command palette and a shortcut. No panel. | On request; on save if you turn it on | No |
| **Lint** | Gutter icon and line tint on the script, message on hover; the Problems dock for the project | The saved script on save (on by default); the project on Check | No |
| **Type check** | Same Problems dock, its own filter | In the background a moment after you save; on Check | No |
| **Tests** | A Tests dock: tree, run, failures. A ▶ in the gutter of each `test_` function; "Run tests" on folders in the FileSystem dock | On request; "rerun on save" if you turn it on | **Yes**: the dock floats well on a second monitor |
| **Code health** | A Health dock you open when you need it (content decided later) | On request, and after a Check | **Yes**: it floats, to have room for the overview |
| **Check** | A button with a status dot in the editor's top bar | On request | No |

### Formatting

- **Format script** formats the editor's text through
  `gdsuite fmt -- -` (stdin), then replaces the text as one undo step,
  keeping the caret and scroll position. It never rewrites a file behind an
  open, unsaved script.
- **Format on save** is off by default; turning it on is one checkbox in the
  GDSuite settings (below).
- A file that would change meaning is left alone, as on the command line; the
  editor says why in a toast, not a dialog.

### Lint and type check: the Problems dock

- One list for both, grouped by script, newest run first. Filters: errors or
  warnings, lint or types, this script or the whole project. A count on the
  dock's tab.
- Click a line to open the script at the line and column.
- Each finding has its rule as a link to its documentation page, and, when it
  can be fixed, a **Fix** button (`lint -- --fix` on that script).
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
  folder, a script, one test; **Rerun failed**; stop.
- A failure shows the message, expected and actual side by side, and the line;
  click to open it.
- In the script editor, a ▶ in the gutter of every `func test_` runs that test,
  and its last result shows as an icon there.
- **Run tests** in the FileSystem dock's context menu, on a folder or a
  script.
- **Rerun on save** (off by default) runs gdtest's watch mode in the
  background while it's on: it reruns the tests of a saved test script, and
  the tests that use a saved script when gdtest can tell. If the watch mode
  turns out to be too heavy to keep running, the dock runs the affected tests
  once per save instead.

### Code health: a dock you open when you need it

Code health is a review, not something to watch while you work, so it does
not get a main screen tab next to 2D, 3D, Script and Game: that place is for
what you use all the time. It is a dock, closed by default, opened from the
GDSuite menu or the command palette, which can float to have room. What it
shows is decided later, with the tool; the starting point:

- the project's grade, and one line per area (a folder or a
  `[health.groups]` group);
- the scripts most worth attention, each number explained in plain words,
  click to open the script;
- the dependency rules between groups, and what breaks them;
- "Open the dependency graph", gdhealth's HTML graph in the browser.

It runs on request, and refreshes after a Check that includes `health`.

### Check: the top bar

A **Check** button in the editor's top bar with a status dot (not run, running,
passed, failed). It runs `[check] steps`, as `gdsuite check` does, and fills
the Problems dock, the Tests dock and the Health screen. Click the dot to open
the first failing step's view.

## Settings

Two places, each for what it's good at, both reachable from the panel's ⚙
button:

**Your preferences: Editor Settings → GDSuite** (per computer, all projects):

| Setting | Default |
|---|---|
| Format on save | Off |
| Lint on save | On |
| Type check after saving | On (in the background) |
| Rerun tests on save | Off |
| Show problems in the gutter | On |
| `gdsuite` program | Found automatically |

Shortcuts (format script, run tests at cursor, rerun failed, check) are
registered with `EditorSettings.add_shortcut()`, so they show up and are
rebound in the editor's Shortcuts tab.

**The project's rules: `gdsuite.toml`**, which the terminal and CI read too.
The panel edits it for the common changes (turn a rule off, the steps of
Check, the test folder), through a `gdsuite` command that keeps the file's
comments, and has **Open gdsuite.toml** for the rest. A team that wants
everyone to format on save says so in `gdsuite.toml`:

```toml
[editor]
format_on_save = true
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
3. **Health.** The dock, once its content is decided.
4. **GDExtension.** The same addon calls gdsuite in-process instead of
   running it, which needs each tool to offer a library API that returns its
   results rather than printing them. Then the Asset Store.

Each phase ships on its own and is usable without the next.

## Decided

- **Godot's own warnings and errors:** the Problems dock shows everything the
  type check reports, including what Godot also shows for open scripts, so
  nothing disappears from the list when a script is opened.
- **Rerun on save:** gdtest's watch mode, with a per-save run as a fallback if
  it proves too heavy.
- **Code health:** a dock opened when needed, not a main screen tab. Its
  content is decided later, with the tool.
