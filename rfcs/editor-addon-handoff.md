# Editor addon: handoff notes

Where the editor work stands, for whoever picks it up next. The design is in
[editor-addon](editor-addon.md); this note covers what is built, what was
checked, and what is left.

## Pull requests

| PR | Branch | Base | What | State |
|---|---|---|---|---|
| #20 | `editor-panel` | `main` | Phase 1: the addon, Check, Problems dock, gutter markers, formatting, settings | Open, CI green |
| #21 | `editor-tests` | `editor-panel` | Phase 2: the Tests dock, gutter ▶, FileSystem "Run Tests", Watch, status icons, retiring `gdtest_editor` | Open, stacked on #20 |
| #22 | `auto/pin-tools` | `main` | The automatic bump to gdtest 0.2.8 | Open |

Merge #20 first, then retarget #21 to `main`. #21 already pins gdtest 0.2.9
(`Cargo.toml` and `.gdsuite-release.toml`), so #22 is redundant once #21 is
in; close it or let it rebase to nothing. Releases are made by hand (see
[RELEASING](../RELEASING.md)); nothing is released from these branches yet.

## Decisions made along the way

- The addon is GDScript and runs the `gdsuite` command line. A GDExtension
  comes later (phase 4).
- Godot 4.7 and 4.8 come first; 4.5 and 4.6 must keep working. Docks use
  `EditorDock` when the class exists (4.6+), through
  `ClassDB.instantiate("EditorDock")`, and the bottom panel otherwise.
- The addon is embedded in the binary (`include_str!` in
  `src/editor_addon.rs`) and written to `addons/gdsuite/` by `gdsuite setup`
  and the first `gdsuite test`.
- The SHA-256 of each file gdsuite wrote is kept in
  `addons/gdsuite/files.sha256`, committed with the addon. On an update or a
  downgrade, a file that still matches its hash (line endings aside) is
  replaced; a file the user changed is kept and reported. Releases 0.7.0 to
  0.7.4 wrote no hashes, so `src/editor_addon_released.sha256` has those of
  their files; nothing is added to it for later releases.
- `addons/gdtest_editor` (the old test panel) is no longer installed. When it
  matches a known gdtest copy (ignoring `.uid` files, line endings and
  plugin.cfg's `version=` line), it is backed up to
  `.gdsuite/backups/gdtest_editor`, removed, and taken out of project.godot's
  enabled plugins. A modified copy is left alone with a warning. See
  `retire_gdtest_editor_addon` in `src/tooling.rs`.
- Watch (once called "rerun on save"): setting `gdsuite/on_save/run_tests`,
  project key `[editor] run_tests_on_save`, off by default. The dock's
  **Watch** button is the same setting. On save, the dock asks
  `gdtest --list --affected-by <file>` which tests the file reaches; Watch
  runs them, and with Watch off they are marked to run (🕑). If gdtest fails,
  the dock falls back to the saved script's own tests.
- Status icons in the editor's top bar: Check (grey until run, then green,
  yellow for warnings, red for errors) and Tests (grey when not run or some
  tests are to run, red on a failure, green when all pass).
- Tests toolbar, after VS Code's Testing view, JetBrains and Vitest UI:
  Run all (Stop while running), Run failed (n), Watch, Search tests, the
  ✖/✔/🕑 counts as filters, Collapse all, a progress bar and
  "Running x / y". A ▶ on each row runs it; the Time title sorts by duration.
  The tree is updated in place, so order and folding stay put during a run.
- Problems shows format findings too (`gdsuite fmt -- --check --format
  json`). Columns in both docks can be resized (`column_resizer.gd`); widths
  are kept in the editor's project metadata.
- Both docks set `auto_translate_mode` to disabled: Godot otherwise
  translated words it knows ("Stop", "Time") into the editor language.
- The Tests dock runs gdtest with the editor's own Godot
  (`--godot-exe OS.get_executable_path()`), `--no-auto-import` and
  `--reporter json`, and passes targets (`res://` folders, files,
  `file::test`) unchanged.

## gdtest changes this needed

Shipped in gdtest 0.2.8 (q-rault/gdtest#16, released):

- A `res://` target that is a folder is walked, as a plain path is.
- A lone `file::test` target only takes the quick single-test run when the
  output is the pretty one and no other run option is given; with
  `--reporter json` it goes through the pool, so the dock gets JSON events.

The workarounds the dock had for both were removed in da806ff.

Shipped in gdtest 0.2.9 (q-rault/gdtest#17, released), for Watch:

- A dependency graph (scripts, scenes, resources, `class_name`, autoloads,
  uids, folders) maps a changed file to the test files that reach it:
  `--affected-by <FILE>`, also used by `--watch` and `--changed-only`.
  `project.godot`, `gdtest.toml`, autoloads and `addons/gdtest/` run
  everything.
- Workers report the scenes and scripts each test file adds to the tree;
  gdtest keeps them in `.godot/gdtest/uses.json` and adds them to the graph,
  so a `load()` with a computed path no longer makes a test run on every
  change once it has been seen. `# gdtest: loads res://…` covers the rest.
- On the 379-test game: a changed script reruns a median 13 of 52 test files
  (39% of test time), against 43 with the static graph alone.

## Things worth knowing

- **Pipes on Windows.** `OS.execute_with_pipe` with a blocking pipe never
  reaches EOF on Windows, so runs never finished. `runner.gd` now opens the
  pipe non-blocking (`execute_with_pipe(path, args, false)`) and one thread
  reads stdout and stderr with `get_buffer`, splits on `\n`, and stops when
  the process has exited and a read comes back empty.
- **JSON events** the dock reads: `test_start`, `test_pass`, `test_fail`,
  `test_error`, `summary`; discovery uses `--list`, which gives
  `{kind: "test", file, name}`.
- `tests_gutter.gd`: `test_at` returns `""` for a gutter of -1, which the
  script editor sends at times.
- Parse JSON with `JSON.new().parse(...)`, not `JSON.parse_string`, to avoid
  error spam on non-JSON lines.

## What was checked

- `cargo test`, clippy, rustfmt, and the addon's own format and lint in CI.
- A headless Godot 4.7 probe on a small project: discovery finds the tests;
  a run gives the right passes and failures, with the failure message kept
  for "rerun failed"; gutter icons are right and a click runs one test; watch
  mode reruns after an edit; stopping leaves no Godot process behind.
  Rechecked after the gdtest 0.2.8 change.
- A real game project (379 tests), cleaned of the old gdsuite setup
  (`[tools]` pins, `.gdsuite/`, `addons/gdtest_editor`) and set up with this
  branch: `gdsuite setup` upgraded `addons/gdtest` from 0.2.1 to 0.2.8 (0.2.9
  since) and
  installed `addons/gdsuite`. `gdsuite test` and the headless editor dock gave
  the same results; the few failures there exist on its main branch with the
  old gdtest too.

- The windowed editor on 4.7, by hand on the game: docks, gutter ▶, Run
  Tests, Shift+Alt+F. Feedback from that round (highlighting, status icons,
  folding during runs, column widths, durations, live progress, format in
  Problems) went into 1a7d918, checked in a windowed probe with screenshots.
- Launchers in `.gdsuite/` were sometimes left empty when two setups ran at
  once; they are now replaced atomically (`refresh_link_to` in
  `src/bundled.rs`, with a test that failed before).

## Not checked yet

- A second hands-on pass on the reworked Tests dock and Watch, after
  `gdsuite setup` with the 0.2.9 build.
- Godot 4.8, and the 4.5 bottom-panel fallback in a window.
- macOS and Linux beyond CI.

## Next steps

1. Try the reworked dock on 4.7 (and 4.8), fix what turns up on #21.
2. Merge #20, retarget and merge #21, close #22.
3. Release gdsuite by hand.
4. Phase 3 (Health dock) waits on deciding its content; phase 4 is the
   GDExtension.

To try the branch in a project: install it with
`cargo install --git https://github.com/q-rault/gdsuite --branch editor-tests --locked --force`,
run `gdsuite setup` in the project, then turn on **GDSuite** in Project
Settings → Plugins. `cargo install gdsuite --locked --force` goes back to the
released version.
