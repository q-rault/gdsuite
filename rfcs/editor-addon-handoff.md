# Editor addon: handoff notes

Where the editor work stands, for whoever picks it up next. The design is in
[editor-addon](editor-addon.md); this note covers what is built, what was
checked, and what is left.

## Pull requests

| PR | Branch | Base | What | State |
|---|---|---|---|---|
| #20 | `editor-panel` | `main` | Phase 1: the addon, Check, Problems dock, gutter markers, formatting, settings | Open, CI green |
| #21 | `editor-tests` | `editor-panel` | Phase 2: the Tests dock, gutter ▶, FileSystem "Run Tests", rerun on save, retiring `gdtest_editor` | Open, stacked on #20 |
| #22 | `auto/pin-tools` | `main` | The automatic bump to gdtest 0.2.8 | Open |

Merge #20 first, then retarget #21 to `main`. #21 already pins gdtest 0.2.8
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
- A copy of what gdsuite wrote is kept in `.gdsuite/state/editor-addon/`. On
  an update, a file that still matches that copy is replaced; a file the user
  changed is kept and reported.
- `addons/gdtest_editor` (the old test panel) is no longer installed. When it
  matches a known gdtest copy (ignoring `.uid` files, line endings and
  plugin.cfg's `version=` line), it is backed up to
  `.gdsuite/backups/gdtest_editor`, removed, and taken out of project.godot's
  enabled plugins. A modified copy is left alone with a warning. See
  `retire_gdtest_editor_addon` in `src/tooling.rs`.
- Rerun on save: setting `gdsuite/on_save/run_tests`, project key
  `[editor] run_tests_on_save`, off by default. The dock's "Rerun on save"
  switch is the same setting.
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
  branch: `gdsuite setup` upgraded `addons/gdtest` from 0.2.1 to 0.2.8 and
  installed `addons/gdsuite`. `gdsuite test` and the headless editor dock gave
  the same results; the few failures there exist on its main branch with the
  old gdtest too.

## Not checked yet

- The windowed editor by hand: how the docks look, clicking the gutter ▶,
  FileSystem "Run Tests", the Check button, Shift+Alt+F, Rerun on save while
  typing.
- Godot 4.8, and the 4.5 bottom-panel fallback in a window.
- macOS and Linux beyond CI.

## Next steps

1. Try the windowed editor on 4.7 (and 4.8), fix what turns up on #21.
2. Merge #20, retarget and merge #21, close #22.
3. Release gdsuite by hand.
4. Phase 3 (Health dock) waits on deciding its content; phase 4 is the
   GDExtension.

To try the branch in a project: install it with
`cargo install --git https://github.com/q-rault/gdsuite --branch editor-tests --locked --force`,
run `gdsuite setup` in the project, then turn on **GDSuite** in Project
Settings → Plugins. `cargo install gdsuite --locked --force` goes back to the
released version.
