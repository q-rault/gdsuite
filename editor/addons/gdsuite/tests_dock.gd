@tool
extends VBoxContainer
## The Tests dock: the project's tests, run all, some, or one, and what
## failed. It runs the gdtest gdsuite keeps in the project, with the editor's
## own Godot, and reads gdtest's JSON events as they come.
##
## The tree is built once and then updated in place, so the order of the
## tests, what is folded, the selection and the scroll stay as they are while
## tests run.

## A test's result changed: the plugin updates the script editor's gutter.
signal results_changed
## Asks the plugin to open a test: `name` empty opens the script.
signal open_requested(file: String, name: String)
## Asks the plugin to open `file` at `line`: where an assertion failed.
signal open_line_requested(file: String, line: int)
## Asks the plugin to run every test, or stop them, as the Tests icon of the
## top bar.
signal test_all_requested
## Tests started or ended.
signal running_changed
signal setting_toggled(key: String)
signal open_config_requested

const Runner = preload("runner.gd")
const TestsWatch = preload("tests_watch.gd")
const Result = preload("test_result.gd")
const ColumnResizer = preload("column_resizer.gd")
const Ui = preload("ui.gd")
const Problem = preload("problem.gd")
const DetailsPane = preload("details_pane.gd")
const Folding = preload("folding.gd")
const Settings = preload("settings.gd")
const SettingsMenu = preload("settings_menu.gd")

## What the toolbar shows for the tests, from `state()`.
const STATE_NOT_RUN := "not_run"
const STATE_RUNNING := "running"
const STATE_FAIL := "fail"
const STATE_PASS := "pass"

const MENU_RUN := 0
const MENU_OPEN := 1
const MENU_RUN_SELECTED := 2

## The ▶ button on each row.
const BUTTON_RUN := 0

## The three kinds of results the toolbar counts, and shows or hides.
const SHOW_FAILED := "failed"
const SHOW_PASSED := "passed"
const SHOW_TO_RUN := "to_run"

const COLUMN_TEST := 0
## What the details say when no test is selected.
const NO_SELECTION := "Select a test to see why it failed. Double-click opens it."
const LOCALS_HINT := (
	"To see the test's variables here, turn on Project Settings → Debug → GDScript → "
	+ "Always Track Local Variables."
)
const WAITING_TIP := (
	"The tests these files affect run once they compile: until then, they could only fail to "
	+ "load."
)
const ON_SAVE_TIP := (
	"On save: when you save a script, a scene or a resource, run the tests it can affect"
)
## The ⚙ menu: titles and settings.
const SETTINGS := [["When you save", [Settings.TEST_ON_SAVE]]]
## Godot reads a test's local variables, for its failures, only with it on.
const LOCALS_SETTING := "debug/settings/gdscript/always_track_local_variables"
const COLUMN_TIME := 1

var runner: Runner
## The folder `gdsuite test` runs, as "res://test/".
var test_folder := "res://test/"
## Runs the tests a saved file affects; the plugin tells it what is saved.
var watch: TestsWatch = TestsWatch.new(self)

## "res://test/a.gd" -> [test name, ...], in discovery order.
var _tests := {}
## "file::name" -> {status, us, failures, stale, in_run}. `stale`: its script,
## or one it may use, changed since it ran. `in_run`: part of the last run.
var _results := {}
var _pid := -1
var _last_failed: PackedStringArray = []
var _counts := {}
var _started_msec := 0
var _finished_msec := 0
var _run_total := 0
var _run_done := 0
var _run_failed := 0
## Targets saved while a run was going: run once it ends.
var _pending: PackedStringArray = []
## Why `_pending` runs, when a save restarted a run for it.
var _pending_reason := ""
## Why the last run started, when it was not asked for: "affected by hud.gd".
var _reason := ""

## "file::name" -> TreeItem, and "file" -> TreeItem.
var _items := {}
var _file_items := {}
## Files the user unfolded. Nothing else unfolds or folds them.
var _unfolded := {}
var _building := false
## The row under the mouse: the only one that shows its ▶ button.
var _hovered_item: TreeItem
var _spinner_frame := 0
## Slowest first, instead of by name: a click on the Time column's title.
var _sort_by_time := false

var _tree: Tree
var _resizer := ColumnResizer.new()
var _details: RichTextLabel
var _pane: DetailsPane
var _status: Label
var _filter: LineEdit
## Runs every test, or stops the tests while they run.
var _run_button: Button
var _rerun_failed: Button
var _on_save: Button
var _collapse: Button
var _progress: ProgressBar
## SHOW_* -> its toggle: it shows the count, and hides those tests when off.
var _show := {}
## SHOW_* -> the count on its toggle.
var _show_count := {}
var _spinner: Timer
var _context_menu: PopupMenu
var _context_target := ""


func _ready() -> void:
	add_child(watch)
	name = "Tests"
	# The addon is in English: Godot would translate the words it knows
	# ("Stop", "Time") and leave the others.
	auto_translate_mode = AUTO_TRANSLATE_MODE_DISABLED
	size_flags_vertical = SIZE_EXPAND_FILL
	custom_minimum_size = Vector2(0, 160)

	# As in the Problems dock: what to show on the left, what to do on the
	# right.
	var bar := Ui.toolbar()
	add_child(bar)
	_show[SHOW_FAILED] = Ui.counted_toggle(
		bar, Ui.icon("ImportFail"), "", "Failed: show the tests that failed", _rebuild
	)
	_show[SHOW_PASSED] = Ui.counted_toggle(
		bar, Ui.icon("ImportCheck"), "", "Passed: show the tests that passed", _rebuild
	)
	_show[SHOW_TO_RUN] = Ui.counted_toggle(
		bar, Ui.icon("Time"), "",
		"To run: show the tests not run yet, or changed since they ran", _rebuild
	)
	for kind: String in _show:
		_show_count[kind] = _show[kind].get_meta("count")
	bar.add_child(VSeparator.new())
	_filter = LineEdit.new()
	_filter.placeholder_text = "Search tests"
	_filter.right_icon = Ui.icon("Search")
	_filter.clear_button_enabled = true
	_filter.custom_minimum_size = Vector2(120 * EditorInterface.get_editor_scale(), 0)
	_filter.text_changed.connect(func(_text: String) -> void: _rebuild())
	bar.add_child(_filter)
	_collapse = Ui.button(bar, "CollapseTree", "", "Fold every script", _collapse_all)
	_status = Label.new()
	_status.size_flags_horizontal = SIZE_EXPAND_FILL
	_status.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_status.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	_status.modulate.a = Ui.FADED_ALPHA
	bar.add_child(_status)
	_on_save = SettingsMenu.on_save_toggle(
		bar, Settings.TEST_ON_SAVE, ON_SAVE_TIP,
		func() -> void:
			Settings.set_personal(Settings.TEST_ON_SAVE, _on_save.button_pressed)
			setting_toggled.emit(Settings.TEST_ON_SAVE)
	)
	var failed_tip := "Run the tests that failed again"
	_rerun_failed = Ui.button(bar, "Reload", "Test failed", failed_tip, rerun_failed)
	var test_all := func() -> void: test_all_requested.emit()
	_run_button = Ui.button(bar, "", "Test all", "", test_all)
	var settings := SettingsMenu.new(SETTINGS)
	settings.setting_toggled.connect(func(key: String) -> void: setting_toggled.emit(key))
	settings.open_config_requested.connect(func() -> void: open_config_requested.emit())
	bar.add_child(settings)

	_progress = ProgressBar.new()
	_progress.show_percentage = false
	_progress.custom_minimum_size = Vector2(0, 4 * EditorInterface.get_editor_scale())
	_progress.visible = false
	add_child(_progress)

	_tree = Tree.new()
	_tree.size_flags_vertical = SIZE_EXPAND_FILL
	_tree.hide_root = true
	_tree.columns = 2
	_tree.column_titles_visible = true
	_tree.set_column_title(COLUMN_TEST, "Test ▾")
	_tree.set_column_title(COLUMN_TIME, "Time")
	_tree.set_column_title_tooltip_text(COLUMN_TEST, "Sort by name")
	_tree.set_column_title_tooltip_text(COLUMN_TIME, "Sort by time, slowest first")
	_tree.set_column_title_alignment(COLUMN_TIME, HORIZONTAL_ALIGNMENT_RIGHT)
	_tree.select_mode = Tree.SELECT_MULTI
	Ui.style_tree(_tree)
	# Only what the user selects is selected: a click on empty space, or
	# Escape, selects nothing.
	_tree.empty_clicked.connect(func(_p: Vector2, _b: int) -> void: _deselect())
	_tree.gui_input.connect(_on_tree_input)
	_tree.mouse_exited.connect(func() -> void: _hover(null))
	_tree.allow_rmb_select = true
	_tree.item_activated.connect(_on_item_activated)
	_tree.multi_selected.connect(func(_i: TreeItem, _c: int, _s: bool) -> void: _show_details())
	_tree.item_mouse_selected.connect(_on_item_mouse_selected)
	_tree.item_collapsed.connect(_on_item_collapsed)
	_tree.button_clicked.connect(_on_row_button)
	_tree.column_title_clicked.connect(_on_title_clicked)
	_resizer.attach(_tree, "tests_columns", { COLUMN_TIME: Result.time_width() })
	_details = RichTextLabel.new()
	_details.selection_enabled = true
	_details.bbcode_enabled = true
	# As in the Problems dock: what failed, at the size of the list; a test's
	# name opens it.
	_details.add_theme_font_size_override("normal_font_size", Ui.font_size())
	_details.meta_clicked.connect(
		func(meta: Variant) -> void:
			var link := str(meta)
			if link.begins_with("line:"):
				var at := link.trim_prefix("line:")
				var colon := at.rfind(":")
				open_line_requested.emit(at.substr(0, colon), int(at.substr(colon + 1)))
			else:
				open_requested.emit(link.get_slice("::", 0), link.get_slice("::", 1))
	)
	_pane = DetailsPane.new(_tree, _details, _details, "tests_details")
	add_child(_pane)
	_show_details()

	_context_menu = PopupMenu.new()
	_context_menu.id_pressed.connect(_on_context_menu)
	add_child(_context_menu)

	_spinner = Timer.new()
	_spinner.wait_time = 0.1
	_spinner.timeout.connect(_on_spinner)
	add_child(_spinner)

	_set_running(false)
	show_settings()


# --- What the plugin calls ----------------------------------------------------


## Finds the project's tests, without running them.
func discover() -> void:
	var gdtest := Runner.gdtest_program()
	if gdtest == "":
		Ui.set_status(_status, "Run `gdsuite test` once to set up the tests")
		return
	var found := {}
	var args := gdtest_args(PackedStringArray(["--list"]), PackedStringArray([test_folder]))
	runner.stream(gdtest, args, add_listed.bind(found), _on_list_done.bind(found))


## Runs `targets` ("res://test/a.gd" or "res://test/a.gd::test_b"), or every
## test when empty. While a run is going, the targets wait for it to end.
func run(targets: PackedStringArray, reason := "") -> void:
	var gdtest := Runner.gdtest_program()
	if gdtest == "":
		return
	if targets.is_empty():
		targets = PackedStringArray([test_folder])
	if is_running():
		for target in targets:
			if not target in _pending:
				_pending.append(target)
		return
	_counts = {}
	_reason = reason
	_started_msec = Time.get_ticks_msec()
	_finished_msec = 0
	_start_run(targets)
	_set_running(true)
	var args := gdtest_args(PackedStringArray(), targets)
	_pid = runner.stream(gdtest, args, _on_event, _on_run_done)
	if _pid < 0:
		_set_running(false)
		Ui.set_status(_status, "gdtest could not start")


func rerun_failed() -> void:
	if not _last_failed.is_empty():
		run(_last_failed)


func stop() -> void:
	_pending.clear()
	runner.kill(_pid)


func is_running() -> bool:
	return _pid > 0


## A setting changed: whether saving a file runs the tests it affects again.
func show_settings() -> void:
	watch.enabled = bool(Settings.value(Settings.TEST_ON_SAVE))
	SettingsMenu.show_on_save(_on_save)


## Stops the run going, which now runs code that changed, and starts another
## with `targets` and the tests the stopped run had yet to finish. As in
## Vitest: saving three times in a row doesn't make three runs, one after the
## other.
func restart(targets: PackedStringArray, reason: String) -> void:
	var next := PackedStringArray()
	if test_folder in targets or test_folder in _pending:
		next.append(test_folder)
	else:
		var unfinished := {}
		for key: String in _results:
			var result: Dictionary = _results[key]
			if result.in_run and result.status in [Result.STATUS_QUEUED, Result.STATUS_RUNNING]:
				var file := key.get_slice("::", 0)
				if not unfinished.has(file):
					unfinished[file] = []
				unfinished[file].append(key)
		for file: String in unfinished:
			if unfinished[file].size() == _tests.get(file, []).size():
				next.append(file)
			else:
				next.append_array(PackedStringArray(unfinished[file]))
		for target in targets + _pending:
			if not target in next:
				next.append(target)
	_pending = next
	_pending_reason = reason
	runner.kill(_pid)


## The project's tests: "res://test/a.gd" -> [test name, ...].
func test_files() -> Dictionary:
	return _tests


## Marks the tests of `found` (file -> [test name]) to run again, and
## returns their keys ("file::name").
func mark_stale(found: Dictionary) -> Array:
	var keys := []
	for file: String in found:
		for test_name: String in found[file]:
			var key := Result.key(file, test_name)
			keys.append(key)
			if _results.has(key):
				_results[key].stale = true
	_refresh_items()
	return keys


## The tests of `keys` are up to date again.
func mark_up_to_date(keys: Array) -> void:
	for key: String in keys:
		if _results.has(key):
			_results[key].stale = false
	_refresh_items()
	update_status()
	results_changed.emit()


## The tests of `file` and their status: [{name, status}].
func tests_of(file: String) -> Array:
	var tests := []
	for test_name: String in _tests.get(file, []):
		var result: Dictionary = _results.get(Result.key(file, test_name), {})
		tests.append({ "name": test_name, "status": result.get("status", Result.STATUS_IDLE) })
	return tests


## How many tests failed, or could not run, the last time they ran.
func failed_count() -> int:
	var failed := 0
	for key: String in _results:
		if _results[key].status in [Result.STATUS_FAIL, Result.STATUS_ERROR]:
			failed += 1
	return failed


## The state of the whole suite, for the toolbar: STATE_FAIL when a test
## fails, else STATE_NOT_RUN when one has not run since it changed, else
## STATE_PASS. STATE_RUNNING while tests run.
func state() -> String:
	if is_running():
		return STATE_RUNNING
	var not_run := false
	var total := 0
	for file: String in _tests:
		for test_name: String in _tests[file]:
			total += 1
			var result: Dictionary = _results.get(Result.key(file, test_name), {})
			var status: String = result.get("status", Result.STATUS_IDLE)
			if status in [Result.STATUS_FAIL, Result.STATUS_ERROR]:
				return STATE_FAIL
			if status == Result.STATUS_IDLE or result.get("stale", false):
				not_run = true
	return STATE_NOT_RUN if not_run or total == 0 else STATE_PASS


## Whether every test passed with the code as it is now: none failed, none
## is to run, and no save is waiting to be looked at.
func all_pass() -> bool:
	return state() == STATE_PASS and watch.is_idle()


## One line on the suite, for the toolbar's tooltip.
func summary() -> String:
	if is_running():
		return "Running %d of %d tests" % [_run_done, _run_total]
	return Result.summary(_tests, _results)


# --- Running ------------------------------------------------------------------


func add_listed(line: String, found: Dictionary) -> void:
	var event := Result.event(line)
	if event.get("kind") != "test":
		return
	if not found.has(event.file):
		found[event.file] = []
	found[event.file].append(event.name)


func _on_list_done(_code: int, found: Dictionary) -> void:
	if found == _tests:
		return
	_tests = found
	_rebuild()
	update_status()
	results_changed.emit()


## gdtest's arguments to run `targets` ("res://" folders, scripts, or
## "file::test") with `options`.
func gdtest_args(options: PackedStringArray, targets: PackedStringArray) -> PackedStringArray:
	var args := PackedStringArray([
		"--godot-exe", OS.get_executable_path(),
		"--project-path", Runner.project_root(),
		"--no-auto-import",
		"--reporter", "json",
	])
	args.append_array(options)
	args.append_array(targets)
	return args


func _on_event(line: String) -> void:
	var event := Result.event(line)
	var kind: String = event.get("kind", "")
	if kind == "summary":
		_counts = event
		return
	var file: String = event.get("file", "")
	var test_name: String = event.get("name", "")
	if file == "" or test_name == "":
		return
	var status := ""
	match kind:
		"test_start":
			status = Result.STATUS_RUNNING
		"test_pass":
			status = Result.STATUS_PASS
		"test_fail":
			status = Result.STATUS_FAIL
		"test_error":
			status = Result.STATUS_ERROR
		"test_skip", "test_pending":
			status = Result.STATUS_SKIP
		_:
			return
	var key := Result.key(file, test_name)
	var known: bool = _tests.has(file) and test_name in _tests[file]
	if not known:
		if not _tests.has(file):
			_tests[file] = []
		_tests[file].append(test_name)
		# A test discovery missed (it was added since): counted in this run.
		if not _results.has(key) or _results[key].status != Result.STATUS_QUEUED:
			_run_total += 1
	var failures: Array = event.get("failures", [])
	if kind == "test_error":
		failures = [str(event.get("message", "the test could not run"))]
	var previous: Dictionary = _results.get(key, {})
	_results[key] = {
		"status": status,
		"us": int((previous if status == Result.STATUS_RUNNING else event).get("us", 0)),
		"failures": failures,
		"details": event.get("details", []),
		"stale": false,
		"in_run": true,
	}
	if status != Result.STATUS_RUNNING:
		_run_done += 1
		if status in [Result.STATUS_FAIL, Result.STATUS_ERROR]:
			_run_failed += 1
	if known:
		_update_item(file, test_name)
	else:
		_rebuild()
	update_status()
	if status != Result.STATUS_RUNNING:
		results_changed.emit()


func _on_run_done(_code: int) -> void:
	_pid = -1
	_finished_msec = Time.get_ticks_msec()
	# Tests still queued or running were stopped, or their worker crashed:
	# they keep the result they had before.
	var interrupted := false
	for key: String in _results:
		var result: Dictionary = _results[key]
		if result.status in [Result.STATUS_QUEUED, Result.STATUS_RUNNING]:
			result.status = result.get("before", Result.STATUS_IDLE)
			result.stale = result.get("stale_before", false)
			result.in_run = false
			interrupted = true
	watch.run_ended(interrupted)
	_last_failed = PackedStringArray()
	for key: String in _results:
		if _results[key].status in [Result.STATUS_FAIL, Result.STATUS_ERROR]:
			_last_failed.append(key)
			# As in Vitest: a script with a test that failed opens, to show it.
			var file := key.get_slice("::", 0)
			_unfolded[file] = true
			if _file_items.has(file):
				_building = true
				_file_items[file].collapsed = false
				_building = false
	_set_running(false)
	_refresh_items()
	_show_details()
	results_changed.emit()
	if not _pending.is_empty():
		var next := _pending
		var reason := _pending_reason
		_pending = PackedStringArray()
		_pending_reason = ""
		run.call_deferred(next, reason)


## Marks the tests of `targets` as queued and the others as not part of this
## run, then shows where the run is.
func _start_run(targets: PackedStringArray) -> void:
	watch.run_starting(targets)
	_run_total = 0
	_run_done = 0
	_run_failed = 0
	for key: String in _results:
		_results[key].in_run = false
	var first: TreeItem = null
	for file: String in _tests:
		for test_name: String in _tests[file]:
			if not Result.in_targets(file, test_name, targets):
				continue
			var key := Result.key(file, test_name)
			var previous: Dictionary = _results.get(key, {})
			_results[key] = {
				"status": Result.STATUS_QUEUED,
				"us": int(previous.get("us", 0)),
				"failures": [],
				"stale": false,
				"in_run": true,
				"before": previous.get("status", Result.STATUS_IDLE),
				"stale_before": previous.get("stale", false),
			}
			_run_total += 1
	_refresh_items()
	_reveal(targets)
	results_changed.emit()


## Scrolls to what a run targets, so it is easy to find: one test is shown
## in its unfolded script. Nothing is selected: only the user selects.
func _reveal(targets: PackedStringArray) -> void:
	if targets.size() != 1:
		return
	var target := targets[0]
	var item: TreeItem = _items.get(target, _file_items.get(target))
	if item == null:
		return
	var parent := item.get_parent()
	if parent != null and parent != _tree.get_root() and parent.collapsed:
		_unfolded[target.get_slice("::", 0)] = true
		_building = true
		parent.collapsed = false
		_building = false
	_tree.scroll_to_item(item)


func _selected_targets() -> PackedStringArray:
	var targets := PackedStringArray()
	var item := _tree.get_next_selected(null)
	while item != null:
		var meta: Dictionary = item.get_metadata(0)
		targets.append(meta.file if meta.name == "" else Result.key(meta.file, meta.name))
		item = _tree.get_next_selected(item)
	return targets


func _on_row_button(item: TreeItem, _column: int, id: int, _mouse_button: int) -> void:
	if id != BUTTON_RUN:
		return
	var meta: Dictionary = item.get_metadata(0)
	run(PackedStringArray([meta.file if meta.name == "" else Result.key(meta.file, meta.name)]))


## Shows the ▶ button on `item`, the row under the mouse, as the Problems
## dock shows its copy button, and takes it off the row before.
func _hover(item: TreeItem) -> void:
	if item == _hovered_item:
		return
	if is_instance_valid(_hovered_item):
		var button := _hovered_item.get_button_by_id(COLUMN_TEST, BUTTON_RUN)
		if button >= 0:
			_hovered_item.erase_button(COLUMN_TEST, button)
	_hovered_item = item
	if item != null:
		var meta: Dictionary = item.get_metadata(0)
		var tip := "Run the tests of this script" if meta.name == "" else "Run this test"
		item.add_button(COLUMN_TEST, Ui.icon("Play"), BUTTON_RUN, false, tip)


func _on_tree_input(event: InputEvent) -> void:
	Folding.on_click(_tree, event)
	if event is InputEventMouseMotion:
		_hover(_tree.get_item_at_position(event.position))
	elif event is InputEventKey and event.pressed and event.keycode == KEY_ESCAPE:
		_deselect()
		_tree.accept_event()


func _deselect() -> void:
	_tree.deselect_all()
	_show_details()


## A click on the Time title sorts by duration, slowest first; on the Test
## title, by name.
func _on_title_clicked(column: int, _mouse_button: int) -> void:
	_sort_by_time = column == COLUMN_TIME
	_tree.set_column_title(COLUMN_TEST, "Test" if _sort_by_time else "Test ▾")
	_tree.set_column_title(COLUMN_TIME, "Time ▾" if _sort_by_time else "Time")
	_rebuild()


func _collapse_all() -> void:
	_building = true
	var folded := Folding.toggle_all(_file_items.values())
	_building = false
	_unfolded.clear()
	if not folded:
		for file: String in _file_items:
			_unfolded[file] = true
	_update_fold_button()


func _update_fold_button() -> void:
	Folding.show_state(_collapse, _file_items.values())


# --- Showing ------------------------------------------------------------------


## Builds the tree again, for new tests or a new filter, keeping what is
## unfolded and selected.
func _rebuild() -> void:
	var selected := {}
	var item := _tree.get_next_selected(null)
	while item != null:
		var meta: Dictionary = item.get_metadata(0)
		selected[Result.key(meta.file, meta.name)] = true
		item = _tree.get_next_selected(item)

	_building = true
	_tree.clear()
	_hovered_item = null
	_items.clear()
	_file_items.clear()
	var root := _tree.create_item()
	var needle := _filter.text.strip_edges().to_lower()
	var hiding := _show.values().any(func(b: Button) -> bool: return not b.button_pressed)
	var filtering := needle != "" or hiding
	var files := _tests.keys()
	files.sort()
	if _sort_by_time:
		files.sort_custom(func(a: String, b: String) -> bool: return _file_us(a) > _file_us(b))
	for file: String in files:
		var names: Array = _tests[file].filter(
			func(n: String) -> bool:
				var found := needle == "" or needle in n.to_lower() or needle in file.to_lower()
				return found and _show[_kind_of(file, n)].button_pressed
		)
		if names.is_empty():
			continue
		if _sort_by_time:
			names.sort_custom(
				func(a: String, b: String) -> bool: return _test_us(file, a) > _test_us(file, b)
			)
		var file_item := _tree.create_item(root)
		file_item.set_metadata(0, { "file": file, "name": "" })
		file_item.set_tooltip_text(COLUMN_TEST, file)
		_file_items[file] = file_item
		for test_name: String in names:
			var test_item := _tree.create_item(file_item)
			test_item.set_text(COLUMN_TEST, test_name.trim_prefix("test_").replace("_", " "))
			test_item.set_metadata(0, { "file": file, "name": test_name })
			_items[Result.key(file, test_name)] = test_item
			if selected.has(Result.key(file, test_name)):
				test_item.select(COLUMN_TEST)
		file_item.collapsed = not (_unfolded.has(file) or filtering)
		if selected.has(Result.key(file, "")):
			file_item.select(COLUMN_TEST)
	_building = false
	_refresh_items()
	_update_fold_button()


## Shows every test's state in the tree as it is, without rebuilding it.
func _refresh_items() -> void:
	for key: String in _items:
		var meta: Dictionary = _items[key].get_metadata(0)
		_show_test(_items[key], meta.file, meta.name)
	for file: String in _file_items:
		_show_file(file)
	_update_counts()


func _update_item(file: String, test_name: String) -> void:
	var item: TreeItem = _items.get(Result.key(file, test_name))
	var hidden_kind: bool = not _show[_kind_of(file, test_name)].button_pressed
	# A test that moves into a hidden kind leaves the list, one that moves
	# out of it comes in.
	if (item != null) == hidden_kind:
		_rebuild()
		return
	if item != null:
		_show_test(item, file, test_name)
	if _file_items.has(file):
		_show_file(file)
	_update_counts()


## Which of the toolbar's kinds a test is: SHOW_FAILED, SHOW_PASSED, or
## SHOW_TO_RUN for a test not run yet, waiting, running, or changed since.
func _kind_of(file: String, test_name: String) -> String:
	var result: Dictionary = _results.get(Result.key(file, test_name), {})
	match result.get("status", Result.STATUS_IDLE):
		Result.STATUS_FAIL, Result.STATUS_ERROR:
			return SHOW_FAILED
		Result.STATUS_PASS, Result.STATUS_SKIP:
			return SHOW_TO_RUN if result.get("stale", false) else SHOW_PASSED
	return SHOW_TO_RUN


## The number of tests of each kind, on the toolbar's toggles.
func _update_counts() -> void:
	var counts := { SHOW_FAILED: 0, SHOW_PASSED: 0, SHOW_TO_RUN: 0 }
	for file: String in _tests:
		for test_name: String in _tests[file]:
			counts[_kind_of(file, test_name)] += 1
	var colors := {
		SHOW_FAILED: Problem.severity_color("error"),
		SHOW_PASSED: Ui.success_color(),
		SHOW_TO_RUN: Ui.text_color(),
	}
	for kind: String in counts:
		Ui.show_count(_show_count[kind], counts[kind], colors[kind])
	_rerun_failed.text = "Test failed"
	if not _last_failed.is_empty():
		_rerun_failed.text += " (%d)" % _last_failed.size()


func _test_us(file: String, test_name: String) -> int:
	return int(_results.get(Result.key(file, test_name), {}).get("us", 0))


func _file_us(file: String) -> int:
	var total := 0
	for test_name: String in _tests[file]:
		total += _test_us(file, test_name)
	return total


func _show_test(item: TreeItem, file: String, test_name: String) -> void:
	var result: Dictionary = _results.get(Result.key(file, test_name), {})
	var status: String = result.get("status", Result.STATUS_IDLE)
	var stale: bool = result.get("stale", false)
	item.set_icon(COLUMN_TEST, Result.icon(status, _spinner_frame))
	item.set_icon_modulate(COLUMN_TEST, Color(1, 1, 1, 0.45 if stale else 1.0))
	item.set_text(COLUMN_TIME, Result.duration(int(result.get("us", 0))))
	item.set_text_alignment(COLUMN_TIME, HORIZONTAL_ALIGNMENT_RIGHT)
	item.set_custom_font(COLUMN_TIME, Ui.mono_font())
	var tooltip := "%s\n%s" % [test_name, Result.describe(status)]
	if stale:
		tooltip += " (changed since it ran)"
	item.set_tooltip_text(COLUMN_TEST, tooltip)
	_paint(item, status, status == Result.STATUS_IDLE or stale)


func _show_file(file: String) -> void:
	var item: TreeItem = _file_items[file]
	var worst := Result.STATUS_IDLE
	var total_us := 0
	var failed := 0
	var stale := false
	for test_name: String in _tests[file]:
		var result: Dictionary = _results.get(Result.key(file, test_name), {})
		var status: String = result.get("status", Result.STATUS_IDLE)
		worst = Result.worse(worst, status)
		total_us += int(result.get("us", 0))
		if status in [Result.STATUS_FAIL, Result.STATUS_ERROR]:
			failed += 1
		stale = stale or result.get("stale", false)
	var text := file.trim_prefix(test_folder).trim_prefix("res://")
	if failed > 0:
		text += "  (%d failed)" % failed
	item.set_text(COLUMN_TEST, text)
	var idle := worst == Result.STATUS_IDLE
	item.set_icon(COLUMN_TEST, Ui.icon("GDScript") if idle else Result.icon(worst, _spinner_frame))
	item.set_icon_modulate(COLUMN_TEST, Color(1, 1, 1, 0.45 if stale else 1.0))
	item.set_text(COLUMN_TIME, Result.duration(total_us))
	item.set_text_alignment(COLUMN_TIME, HORIZONTAL_ALIGNMENT_RIGHT)
	item.set_custom_font(COLUMN_TIME, Ui.mono_font())
	_paint(item, worst, false)


## Colours a row's text, as the Problems dock does: a test that failed is in
## the colour of an error, one not run since it changed is faded, and the
## time is faded, as secondary.
func _paint(item: TreeItem, status: String, dim: bool) -> void:
	var color := Ui.text_color()
	if status in [Result.STATUS_FAIL, Result.STATUS_ERROR]:
		color = Problem.severity_color("error")
	if dim:
		color = Ui.faded(color)
	item.set_custom_color(COLUMN_TEST, color)
	item.set_custom_color(COLUMN_TIME, Ui.faded(color))


## Under the list: why the selected tests failed (see `Result.failure_lines`).
func _show_details() -> void:
	var dim := Ui.faded(Ui.text_color()).to_html()
	var item := _tree.get_selected()
	if item == null:
		_details.text = "[color=#%s]%s[/color]" % [dim, NO_SELECTION]
		_pane.refresh_summary()
		return
	var meta: Dictionary = item.get_metadata(0)
	var lines := PackedStringArray()
	var names: Array = [meta.name] if meta.name != "" else _tests.get(meta.file, [])
	var without_locals := false
	for test_name: String in names:
		var result: Dictionary = _results.get(Result.key(meta.file, test_name), {})
		if result.get("failures", []).is_empty():
			continue
		if not lines.is_empty():
			lines.append("")
		lines.append_array(Result.failure_lines(meta.file, test_name, result))
		without_locals = without_locals or Result.lacks_locals(result)
	if lines.is_empty():
		var result: Dictionary = _results.get(Result.key(meta.file, meta.name), {})
		var what := Result.describe(result.get("status", Result.STATUS_IDLE)) if meta.name != "" \
				else "%d test%s" % [names.size(), "" if names.size() == 1 else "s"]
		lines.append("[color=#%s]%s · %s[/color]" % [dim, meta.file, what])
	elif without_locals and not ProjectSettings.get_setting(LOCALS_SETTING, false):
		lines.append("")
		lines.append("[color=#%s]%s[/color]" % [dim, LOCALS_HINT])
	_details.text = "\n".join(lines)
	_pane.refresh_summary()


func update_status() -> void:
	if is_running():
		var seconds := (Time.get_ticks_msec() - _started_msec) / 1000.0
		# The failures have their count on the left.
		var text := "%d/%d · %.1f s" % [_run_done, _run_total, seconds]
		if _reason != "":
			text += " · " + _reason
		Ui.set_status(_status, text)
		_progress.max_value = maxi(_run_total, 1)
		_progress.value = _run_done
		Ui.paint_progress(_progress, _run_failed > 0)
		return
	var total := 0
	for file: String in _tests:
		total += _tests[file].size()
	var blocked := watch.blocked()
	if not blocked.is_empty():
		var names := ", ".join(blocked.map(func(path: String) -> String: return path.get_file()))
		Ui.set_status(_status, "Waiting for %s to compile" % names)
		_status.tooltip_text = WAITING_TIP
		return
	if _counts.is_empty():
		Ui.set_status(_status, "%d tests" % total)
		return
	var ran := 0
	for count: String in ["pass", "fail", "error", "skipped", "pending"]:
		ran += int(_counts.get(count, 0))
	var seconds := (_finished_msec - _started_msec) / 1000.0
	var text := "%d tests · %.1f s" % [ran, seconds]
	if _reason != "":
		text += " · " + _reason
	Ui.set_status(_status, text)
	_status.tooltip_text = "The last run: " + text


func _set_running(running: bool) -> void:
	_run_button.text = "Stop" if running else "Test all"
	_run_button.icon = Ui.icon("Stop") if running else Ui.svg_icon(Ui.TESTS_ICON)
	_run_button.tooltip_text = "Stop the tests" if running else "Run every test"
	_rerun_failed.disabled = running or _last_failed.is_empty()
	_progress.visible = running
	_update_counts()
	# Not while the dock leaves the editor.
	if running and _spinner.is_inside_tree():
		_spinner.start()
	else:
		_spinner.stop()
	update_status()
	running_changed.emit()


func _on_spinner() -> void:
	_spinner_frame = (_spinner_frame + 1) % 8
	var icon := Result.icon(Result.STATUS_RUNNING, _spinner_frame)
	var running_files := {}
	for key: String in _items:
		if _results.get(key, {}).get("status") == Result.STATUS_RUNNING:
			_items[key].set_icon(COLUMN_TEST, icon)
			running_files[key.get_slice("::", 0)] = true
	for file: String in running_files:
		if _file_items.has(file):
			_show_file(file)
	update_status()


func _on_item_collapsed(item: TreeItem) -> void:
	_update_fold_button.call_deferred()
	if _building:
		return
	var meta: Variant = item.get_metadata(0)
	if not meta is Dictionary or meta.name != "":
		return
	if item.collapsed:
		_unfolded.erase(meta.file)
	else:
		_unfolded[meta.file] = true


func _on_item_activated() -> void:
	var item := _tree.get_selected()
	if item != null:
		var meta: Dictionary = item.get_metadata(0)
		open_requested.emit(meta.file, meta.name)


func _on_item_mouse_selected(_position: Vector2, button: int) -> void:
	if button != MOUSE_BUTTON_RIGHT:
		return
	var item := _tree.get_selected()
	if item == null:
		return
	var meta: Dictionary = item.get_metadata(0)
	_context_target = meta.file if meta.name == "" else Result.key(meta.file, meta.name)
	_context_menu.clear()
	var selected := _selected_targets().size()
	if selected > 1:
		_context_menu.add_item("Run the %d selected" % selected, MENU_RUN_SELECTED)
	else:
		_context_menu.add_item("Run", MENU_RUN)
	_context_menu.add_item("Open", MENU_OPEN)
	_context_menu.position = Vector2i(get_screen_position() + get_local_mouse_position())
	_context_menu.reset_size()
	_context_menu.popup()


func _on_context_menu(id: int) -> void:
	match id:
		MENU_RUN:
			run(PackedStringArray([_context_target]))
		MENU_RUN_SELECTED:
			run(_selected_targets())
		MENU_OPEN:
			open_requested.emit(
				_context_target.get_slice("::", 0),
				_context_target.get_slice("::", 1) if "::" in _context_target else ""
			)
