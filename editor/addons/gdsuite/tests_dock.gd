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
signal watch_toggled(enabled: bool)

const Runner = preload("runner.gd")
const ColumnResizer = preload("column_resizer.gd")

const STATUS_IDLE := "idle"
const STATUS_QUEUED := "queued"
const STATUS_RUNNING := "running"
const STATUS_PASS := "pass"
const STATUS_FAIL := "fail"
const STATUS_ERROR := "error"
const STATUS_SKIP := "skip"

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
const COLUMN_TIME := 1

var runner: Runner
## The folder `gdsuite test` runs, as "res://test/".
var test_folder := "res://test/"

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
var _watch_on := false
## When each file was last saved, in msec: a save reported twice counts once.
var _saved_at := {}

## "file::name" -> TreeItem, and "file" -> TreeItem.
var _items := {}
var _file_items := {}
## Files the user unfolded. Nothing else unfolds or folds them.
var _unfolded := {}
var _building := false
var _spinner_frame := 0
## Slowest first, instead of by name: a click on the Time column's title.
var _sort_by_time := false

var _tree: Tree
var _resizer := ColumnResizer.new()
var _details: RichTextLabel
var _status: Label
var _filter: LineEdit
## Runs every test, or stops the tests while they run.
var _run_button: Button
var _rerun_failed: Button
var _watch: Button
var _collapse: Button
var _progress: ProgressBar
## SHOW_* -> its toggle: it shows the count, and hides those tests when off.
var _show := {}
var _spinner: Timer
var _context_menu: PopupMenu
var _context_target := ""


func _ready() -> void:
	name = "Tests"
	# The addon is in English: Godot would translate the words it knows
	# ("Stop", "Time") and leave the others.
	auto_translate_mode = AUTO_TRANSLATE_MODE_DISABLED
	size_flags_vertical = SIZE_EXPAND_FILL
	custom_minimum_size = Vector2(0, 160)

	var bar := HBoxContainer.new()
	add_child(bar)
	_run_button = _button(bar, "Play", "Run all", "", _on_run_button)
	var failed_tip := "Run the tests that failed again"
	_rerun_failed = _button(bar, "Reload", "Run failed", failed_tip, rerun_failed)
	bar.add_child(VSeparator.new())
	_watch = _button(
		bar,
		"GuiVisibilityVisible",
		"Watch",
		"Watch: when you save a script, a scene or a resource, run the tests it can "
		+ "affect again",
		func() -> void: pass,
	)
	_watch.toggle_mode = true
	_watch.toggled.connect(func(on: bool) -> void: watch_toggled.emit(on))
	bar.add_child(VSeparator.new())
	_filter = LineEdit.new()
	_filter.placeholder_text = "Search tests"
	_filter.right_icon = _icon("Search")
	_filter.clear_button_enabled = true
	_filter.custom_minimum_size = Vector2(160 * EditorInterface.get_editor_scale(), 0)
	_filter.text_changed.connect(func(_text: String) -> void: _rebuild())
	bar.add_child(_filter)
	_show[SHOW_FAILED] = _toggle(bar, "StatusError", "Failed: click to show or hide them")
	_show[SHOW_PASSED] = _toggle(bar, "StatusSuccess", "Passed: click to show or hide them")
	_show[SHOW_TO_RUN] = _toggle(
		bar, "Time", "Not run yet, or changed since they ran: click to show or hide them"
	)
	_collapse = _button(bar, "CollapseTree", "", "Fold every script", _collapse_all)
	_status = Label.new()
	_status.size_flags_horizontal = SIZE_EXPAND_FILL
	_status.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_status.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	bar.add_child(_status)

	_progress = ProgressBar.new()
	_progress.show_percentage = false
	_progress.custom_minimum_size = Vector2(0, 4 * EditorInterface.get_editor_scale())
	_progress.visible = false
	add_child(_progress)

	var split := VSplitContainer.new()
	split.size_flags_vertical = SIZE_EXPAND_FILL
	add_child(split)
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
	_tree.allow_rmb_select = true
	_tree.item_activated.connect(_on_item_activated)
	_tree.multi_selected.connect(func(_i: TreeItem, _c: int, _s: bool) -> void: _show_details())
	_tree.item_mouse_selected.connect(_on_item_mouse_selected)
	_tree.item_collapsed.connect(_on_item_collapsed)
	_tree.button_clicked.connect(_on_row_button)
	_tree.column_title_clicked.connect(_on_title_clicked)
	split.add_child(_tree)
	_resizer.attach(_tree, "tests_columns", { COLUMN_TIME: _time_width() })
	_details = RichTextLabel.new()
	_details.custom_minimum_size = Vector2(0, 60)
	_details.selection_enabled = true
	_details.bbcode_enabled = false
	split.add_child(_details)

	_context_menu = PopupMenu.new()
	_context_menu.id_pressed.connect(_on_context_menu)
	add_child(_context_menu)

	_spinner = Timer.new()
	_spinner.wait_time = 0.1
	_spinner.timeout.connect(_on_spinner)
	add_child(_spinner)

	_set_running(false)


# --- What the plugin calls ----------------------------------------------------


## Finds the project's tests, without running them.
func discover() -> void:
	var gdtest := Runner.gdtest_program()
	if gdtest == "":
		_status.text = "Run `gdsuite test` once to set up the tests"
		return
	var found := {}
	var args := _gdtest_args(PackedStringArray(["--list"]), PackedStringArray([test_folder]))
	runner.stream(gdtest, args, _on_listed.bind(found), _on_list_done.bind(found))


## Runs `targets` ("res://test/a.gd" or "res://test/a.gd::test_b"), or every
## test when empty. While a run is going, the targets wait for it to end.
func run(targets: PackedStringArray) -> void:
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
	_started_msec = Time.get_ticks_msec()
	_finished_msec = 0
	_details.text = ""
	_start_run(targets)
	_set_running(true)
	var args := _gdtest_args(PackedStringArray(), targets)
	_pid = runner.stream(gdtest, args, _on_event, _on_run_done)
	if _pid < 0:
		_set_running(false)
		_status.text = "gdtest could not start"


func rerun_failed() -> void:
	if not _last_failed.is_empty():
		run(_last_failed)


func stop() -> void:
	_pending.clear()
	runner.kill(_pid)


func is_running() -> bool:
	return _pid > 0


## Whether saving a file runs the tests it affects again.
func set_watch(enabled: bool) -> void:
	_watch_on = enabled
	_watch.set_pressed_no_signal(enabled)


## A file of the project was saved: the tests it can affect are out of
## date, and run again when Watch is on. gdtest says which: the
## tests that use the file, directly or through other scripts, scenes and
## resources.
func file_saved(path: String) -> void:
	var gdtest := Runner.gdtest_program()
	if gdtest == "":
		return
	# A scene saved can be reported twice: as a resource and as a scene.
	var now := Time.get_ticks_msec()
	if now - int(_saved_at.get(path, -10000)) < 1000:
		return
	_saved_at[path] = now
	var found := {}
	var options := PackedStringArray(["--list", "--affected-by", path])
	var args := _gdtest_args(options, PackedStringArray([test_folder]))
	if runner.stream(gdtest, args, _on_listed.bind(found), _on_affected.bind(path, found)) < 0:
		_on_affected(1, path, found)


## gdtest listed the tests `path` affects in `found`: file -> [test name].
func _on_affected(code: int, path: String, found: Dictionary) -> void:
	if code != 0 and found.is_empty():
		# A gdtest older than --affected-by: a test script affects its own
		# tests, any other file every test.
		for file: String in _tests:
			if file == path or not _tests.has(path):
				found[file] = _tests[file]
	if found.is_empty():
		return
	for file: String in found:
		for test_name: String in found[file]:
			var key := _key(file, test_name)
			if _results.has(key):
				_results[key].stale = true
	_refresh_items()
	if not _watch_on:
		results_changed.emit()
		return
	if found.size() == _tests.size():
		run(PackedStringArray([test_folder]))
	else:
		run(PackedStringArray(found.keys()))


## The tests of `file` and their status: [{name, status}].
func tests_of(file: String) -> Array:
	var tests := []
	for test_name: String in _tests.get(file, []):
		var result: Dictionary = _results.get(_key(file, test_name), {})
		tests.append({ "name": test_name, "status": result.get("status", STATUS_IDLE) })
	return tests


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
			var result: Dictionary = _results.get(_key(file, test_name), {})
			var status: String = result.get("status", STATUS_IDLE)
			if status in [STATUS_FAIL, STATUS_ERROR]:
				return STATE_FAIL
			if status == STATUS_IDLE or result.get("stale", false):
				not_run = true
	return STATE_NOT_RUN if not_run or total == 0 else STATE_PASS


## One line on the suite, for the toolbar's tooltip.
func summary() -> String:
	if is_running():
		return "Running %d of %d tests" % [_run_done, _run_total]
	var passed := 0
	var failed := 0
	var never_run := 0
	var changed := 0
	var total := 0
	for file: String in _tests:
		for test_name: String in _tests[file]:
			total += 1
			var result: Dictionary = _results.get(_key(file, test_name), {})
			match result.get("status", STATUS_IDLE):
				STATUS_PASS, STATUS_SKIP:
					passed += 1
				STATUS_FAIL, STATUS_ERROR:
					failed += 1
				_:
					never_run += 1
			if result.get("stale", false):
				changed += 1
	var text := "%d tests: %d passed, %d failed, %d not run" % [total, passed, failed, never_run]
	if changed > 0:
		text += "\n%d to run again: their scripts changed since they ran" % changed
	return text


# --- Running ------------------------------------------------------------------


func _on_listed(line: String, found: Dictionary) -> void:
	var event := _event(line)
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
	_update_status()
	results_changed.emit()


## gdtest's arguments to run `targets` ("res://" folders, scripts, or
## "file::test") with `options`.
func _gdtest_args(options: PackedStringArray, targets: PackedStringArray) -> PackedStringArray:
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
	var event := _event(line)
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
			status = STATUS_RUNNING
		"test_pass":
			status = STATUS_PASS
		"test_fail":
			status = STATUS_FAIL
		"test_error":
			status = STATUS_ERROR
		"test_skip", "test_pending":
			status = STATUS_SKIP
		_:
			return
	var key := _key(file, test_name)
	var known: bool = _tests.has(file) and test_name in _tests[file]
	if not known:
		if not _tests.has(file):
			_tests[file] = []
		_tests[file].append(test_name)
		# A test discovery missed (it was added since): counted in this run.
		if not _results.has(key) or _results[key].status != STATUS_QUEUED:
			_run_total += 1
	var failures: Array = event.get("failures", [])
	if kind == "test_error":
		failures = [str(event.get("message", "the test could not run"))]
	var previous: Dictionary = _results.get(key, {})
	_results[key] = {
		"status": status,
		"us": int(event.get("us", 0)) if status != STATUS_RUNNING else int(previous.get("us", 0)),
		"failures": failures,
		"stale": false,
		"in_run": true,
	}
	if status != STATUS_RUNNING:
		_run_done += 1
		if status in [STATUS_FAIL, STATUS_ERROR]:
			_run_failed += 1
	if known:
		_update_item(file, test_name)
	else:
		_rebuild()
	_update_status()
	if status != STATUS_RUNNING:
		results_changed.emit()


func _on_run_done(_code: int) -> void:
	_pid = -1
	_finished_msec = Time.get_ticks_msec()
	# Tests still queued or running were stopped, or their worker crashed:
	# they keep the result they had before.
	for key: String in _results:
		var result: Dictionary = _results[key]
		if result.status in [STATUS_QUEUED, STATUS_RUNNING]:
			result.status = result.get("before", STATUS_IDLE)
			result.stale = result.get("stale_before", false)
			result.in_run = false
	_last_failed = PackedStringArray()
	for key: String in _results:
		if _results[key].status in [STATUS_FAIL, STATUS_ERROR]:
			_last_failed.append(key)
	_set_running(false)
	_refresh_items()
	_show_details()
	results_changed.emit()
	if not _pending.is_empty():
		var next := _pending
		_pending = PackedStringArray()
		run.call_deferred(next)


## Marks the tests of `targets` as queued and the others as not part of this
## run, then shows where the run is.
func _start_run(targets: PackedStringArray) -> void:
	_run_total = 0
	_run_done = 0
	_run_failed = 0
	for key: String in _results:
		_results[key].in_run = false
	var first: TreeItem = null
	for file: String in _tests:
		for test_name: String in _tests[file]:
			if not _in_targets(file, test_name, targets):
				continue
			var key := _key(file, test_name)
			var previous: Dictionary = _results.get(key, {})
			_results[key] = {
				"status": STATUS_QUEUED,
				"us": int(previous.get("us", 0)),
				"failures": [],
				"stale": false,
				"in_run": true,
				"before": previous.get("status", STATUS_IDLE),
				"stale_before": previous.get("stale", false),
			}
			_run_total += 1
	_refresh_items()
	_reveal(targets)
	results_changed.emit()


func _in_targets(file: String, test_name: String, targets: PackedStringArray) -> bool:
	for target in targets:
		var target_file := target.get_slice("::", 0)
		var only := target.get_slice("::", 1) if "::" in target else ""
		var folder := target_file.trim_suffix("/") + "/"
		if (file == target_file or file.begins_with(folder)) and (only == "" or only == test_name):
			return true
	return false


## Selects what a run targets, so it is easy to find: one test is shown in
## its unfolded script, a script is selected.
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
	_tree.deselect_all()
	item.select(COLUMN_TEST)
	_tree.scroll_to_item(item)


func _selected_targets() -> PackedStringArray:
	var targets := PackedStringArray()
	var item := _tree.get_next_selected(null)
	while item != null:
		var meta: Dictionary = item.get_metadata(0)
		targets.append(meta.file if meta.name == "" else _key(meta.file, meta.name))
		item = _tree.get_next_selected(item)
	return targets


func _on_run_button() -> void:
	if is_running():
		stop()
	else:
		run(PackedStringArray())


func _on_row_button(item: TreeItem, _column: int, id: int, _mouse_button: int) -> void:
	if id != BUTTON_RUN:
		return
	var meta: Dictionary = item.get_metadata(0)
	run(PackedStringArray([meta.file if meta.name == "" else _key(meta.file, meta.name)]))


## A click on the Time title sorts by duration, slowest first; on the Test
## title, by name.
func _on_title_clicked(column: int, _mouse_button: int) -> void:
	_sort_by_time = column == COLUMN_TIME
	_tree.set_column_title(COLUMN_TEST, "Test" if _sort_by_time else "Test ▾")
	_tree.set_column_title(COLUMN_TIME, "Time ▾" if _sort_by_time else "Time")
	_rebuild()


func _collapse_all() -> void:
	_unfolded.clear()
	_building = true
	for file: String in _file_items:
		_file_items[file].collapsed = true
	_building = false


# --- Showing ------------------------------------------------------------------


## Builds the tree again, for new tests or a new filter, keeping what is
## unfolded and selected.
func _rebuild() -> void:
	var selected := {}
	var item := _tree.get_next_selected(null)
	while item != null:
		var meta: Dictionary = item.get_metadata(0)
		selected[_key(meta.file, meta.name)] = true
		item = _tree.get_next_selected(item)

	_building = true
	_tree.clear()
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
	var run_icon := _icon("Play")
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
		var tip := "Run the tests of this script"
		file_item.add_button(COLUMN_TEST, run_icon, BUTTON_RUN, false, tip)
		_file_items[file] = file_item
		for test_name: String in names:
			var test_item := _tree.create_item(file_item)
			test_item.set_text(COLUMN_TEST, test_name.trim_prefix("test_").replace("_", " "))
			test_item.set_metadata(0, { "file": file, "name": test_name })
			test_item.add_button(COLUMN_TEST, run_icon, BUTTON_RUN, false, "Run this test")
			_items[_key(file, test_name)] = test_item
			if selected.has(_key(file, test_name)):
				test_item.select(COLUMN_TEST)
		file_item.collapsed = not (_unfolded.has(file) or filtering)
		if selected.has(_key(file, "")):
			file_item.select(COLUMN_TEST)
	_building = false
	_refresh_items()


## Shows every test's state in the tree as it is, without rebuilding it.
func _refresh_items() -> void:
	for key: String in _items:
		var meta: Dictionary = _items[key].get_metadata(0)
		_show_test(_items[key], meta.file, meta.name)
	for file: String in _file_items:
		_show_file(file)
	_update_counts()


func _update_item(file: String, test_name: String) -> void:
	var item: TreeItem = _items.get(_key(file, test_name))
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
	var result: Dictionary = _results.get(_key(file, test_name), {})
	match result.get("status", STATUS_IDLE):
		STATUS_FAIL, STATUS_ERROR:
			return SHOW_FAILED
		STATUS_PASS, STATUS_SKIP:
			return SHOW_TO_RUN if result.get("stale", false) else SHOW_PASSED
	return SHOW_TO_RUN


## The number of tests of each kind, on the toolbar's toggles.
func _update_counts() -> void:
	var counts := { SHOW_FAILED: 0, SHOW_PASSED: 0, SHOW_TO_RUN: 0 }
	for file: String in _tests:
		for test_name: String in _tests[file]:
			counts[_kind_of(file, test_name)] += 1
	for kind: String in counts:
		_show[kind].text = str(counts[kind])
	_rerun_failed.text = "Run failed"
	if not _last_failed.is_empty():
		_rerun_failed.text += " (%d)" % _last_failed.size()


func _test_us(file: String, test_name: String) -> int:
	return int(_results.get(_key(file, test_name), {}).get("us", 0))


func _file_us(file: String) -> int:
	var total := 0
	for test_name: String in _tests[file]:
		total += _test_us(file, test_name)
	return total


func _show_test(item: TreeItem, file: String, test_name: String) -> void:
	var result: Dictionary = _results.get(_key(file, test_name), {})
	var status: String = result.get("status", STATUS_IDLE)
	var stale: bool = result.get("stale", false)
	item.set_icon(COLUMN_TEST, _status_icon(status))
	item.set_icon_modulate(COLUMN_TEST, Color(1, 1, 1, 0.45 if stale else 1.0))
	item.set_text(COLUMN_TIME, _duration(int(result.get("us", 0))))
	item.set_text_alignment(COLUMN_TIME, HORIZONTAL_ALIGNMENT_RIGHT)
	item.set_custom_font(COLUMN_TIME, _mono_font())
	var tooltip := "%s\n%s" % [test_name, _describe(status)]
	if stale:
		tooltip += " (changed since it ran)"
	item.set_tooltip_text(COLUMN_TEST, tooltip)
	_show_in_run(item, result.get("in_run", false), status in [STATUS_IDLE] or stale)


func _show_file(file: String) -> void:
	var item: TreeItem = _file_items[file]
	var worst := STATUS_IDLE
	var total_us := 0
	var failed := 0
	var in_run := false
	var stale := false
	for test_name: String in _tests[file]:
		var result: Dictionary = _results.get(_key(file, test_name), {})
		var status: String = result.get("status", STATUS_IDLE)
		worst = _worse(worst, status)
		total_us += int(result.get("us", 0))
		if status in [STATUS_FAIL, STATUS_ERROR]:
			failed += 1
		in_run = in_run or result.get("in_run", false)
		stale = stale or result.get("stale", false)
	var text := file.trim_prefix(test_folder).trim_prefix("res://")
	if failed > 0:
		text += "  (%d failed)" % failed
	item.set_text(COLUMN_TEST, text)
	item.set_icon(COLUMN_TEST, _status_icon(worst) if worst != STATUS_IDLE else _icon("GDScript"))
	item.set_icon_modulate(COLUMN_TEST, Color(1, 1, 1, 0.45 if stale else 1.0))
	item.set_text(COLUMN_TIME, _duration(total_us))
	item.set_text_alignment(COLUMN_TIME, HORIZONTAL_ALIGNMENT_RIGHT)
	item.set_custom_font(COLUMN_TIME, _mono_font())
	_show_in_run(item, in_run, false)


## Tests of the last run get a tinted row; tests that have not run since
## they changed are dimmed.
func _show_in_run(item: TreeItem, in_run: bool, dim: bool) -> void:
	var theme := EditorInterface.get_editor_theme()
	for column in [COLUMN_TEST, COLUMN_TIME]:
		if in_run:
			var tint := theme.get_color("accent_color", "Editor")
			tint.a = 0.12
			item.set_custom_bg_color(column, tint)
		else:
			item.clear_custom_bg_color(column)
		if dim:
			item.set_custom_color(column, theme.get_color("font_disabled_color", "Editor"))
		else:
			item.clear_custom_color(column)


func _show_details() -> void:
	var item := _tree.get_selected()
	if item == null:
		return
	var meta: Dictionary = item.get_metadata(0)
	var lines := PackedStringArray()
	var names: Array = [meta.name] if meta.name != "" else _tests.get(meta.file, [])
	for test_name: String in names:
		var result: Dictionary = _results.get(_key(meta.file, test_name), {})
		for failure: Variant in result.get("failures", []):
			lines.append("%s: %s" % [test_name, str(failure)])
	_details.text = "\n".join(lines)


func _update_status() -> void:
	if is_running():
		var seconds := (Time.get_ticks_msec() - _started_msec) / 1000.0
		var text := "Running %d / %d" % [_run_done, _run_total]
		if _run_failed > 0:
			text += ", %d failed" % _run_failed
		_status.text = "%s · %.1f s" % [text, seconds]
		_progress.max_value = maxi(_run_total, 1)
		_progress.value = _run_done
		_paint_progress(_run_failed > 0)
		return
	var total := 0
	for file: String in _tests:
		total += _tests[file].size()
	if _counts.is_empty():
		_status.text = "%d tests" % total
		return
	var ran := 0
	for count: String in ["pass", "fail", "error", "skipped", "pending"]:
		ran += int(_counts.get(count, 0))
	var seconds := (_finished_msec - _started_msec) / 1000.0
	_status.text = "%d tests · %.1f s" % [ran, seconds]
	_status.tooltip_text = "The last run: %d tests in %.1f s" % [ran, seconds]


## The progress bar fills green, or red once a test failed.
func _paint_progress(failed: bool) -> void:
	var theme := EditorInterface.get_editor_theme()
	var fill := StyleBoxFlat.new()
	fill.bg_color = theme.get_color("error_color" if failed else "success_color", "Editor")
	_progress.add_theme_stylebox_override("fill", fill)


func _set_running(running: bool) -> void:
	_run_button.text = "Stop" if running else "Run all"
	_run_button.icon = _icon("Stop" if running else "Play")
	_run_button.tooltip_text = "Stop the tests" if running else "Run every test"
	_rerun_failed.disabled = running or _last_failed.is_empty()
	_progress.visible = running
	_update_counts()
	if running:
		_spinner.start()
	else:
		_spinner.stop()
	_update_status()


func _on_spinner() -> void:
	_spinner_frame = (_spinner_frame + 1) % 8
	var icon := _status_icon(STATUS_RUNNING)
	var running_files := {}
	for key: String in _items:
		if _results.get(key, {}).get("status") == STATUS_RUNNING:
			_items[key].set_icon(COLUMN_TEST, icon)
			running_files[key.get_slice("::", 0)] = true
	for file: String in running_files:
		if _file_items.has(file):
			_show_file(file)
	_update_status()


func _on_item_collapsed(item: TreeItem) -> void:
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
	_context_target = meta.file if meta.name == "" else _key(meta.file, meta.name)
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


# --- Helpers ------------------------------------------------------------------


static func _key(file: String, test_name: String) -> String:
	return "%s::%s" % [file, test_name]


func _event(line: String) -> Dictionary:
	var trimmed := line.strip_edges()
	if not trimmed.begins_with("{"):
		return {}
	# JSON.new().parse, unlike JSON.parse_string, prints nothing on bad input.
	var json := JSON.new()
	if json.parse(trimmed) != OK:
		return {}
	return json.data if json.data is Dictionary else {}


func _worse(a: String, b: String) -> String:
	var order := [STATUS_IDLE, STATUS_SKIP, STATUS_PASS, STATUS_QUEUED, STATUS_RUNNING]
	order.append_array([STATUS_FAIL, STATUS_ERROR])
	return a if order.find(a) >= order.find(b) else b


func _describe(status: String) -> String:
	var descriptions := {
		STATUS_PASS: "Passed",
		STATUS_FAIL: "Failed",
		STATUS_ERROR: "Could not run",
		STATUS_SKIP: "Skipped",
		STATUS_QUEUED: "Waiting to run",
		STATUS_RUNNING: "Running",
	}
	return descriptions.get(status, "Not run yet")


func _status_icon(status: String) -> Texture2D:
	match status:
		STATUS_PASS:
			return _icon("StatusSuccess")
		STATUS_FAIL, STATUS_ERROR:
			return _icon("StatusError")
		STATUS_SKIP:
			return _icon("StatusWarning")
		STATUS_QUEUED:
			return _icon("Time")
		STATUS_RUNNING:
			return _icon("Progress%d" % (_spinner_frame + 1))
	return null


## "850 µs", "12 ms", "1.24 s", "2 m 05 s": short, and of one width at a
## glance, to compare.
func _duration(us: int) -> String:
	if us <= 0:
		return ""
	if us < 1000:
		return "%d µs" % us
	if us < 1000000:
		return "%d ms" % roundi(us / 1000.0)
	if us < 60000000:
		return "%.2f s" % (us / 1000000.0)
	var seconds := roundi(us / 1000000.0)
	return "%d m %02d s" % [seconds / 60, seconds % 60]


## Wide enough for the longest duration, at the editor's scale.
func _time_width() -> int:
	var font := _mono_font()
	var size := EditorInterface.get_editor_theme().get_font_size("main_size", "EditorFonts")
	var text_width := font.get_string_size("00 m 00 s", HORIZONTAL_ALIGNMENT_LEFT, -1, size).x
	return int(text_width + 24 * EditorInterface.get_editor_scale())


func _mono_font() -> Font:
	return EditorInterface.get_editor_theme().get_font("source", "EditorFonts")


func _button(
	bar: HBoxContainer, icon_name: String, text: String, tooltip: String, pressed: Callable
) -> Button:
	var button := Button.new()
	button.flat = true
	button.icon = _icon(icon_name)
	button.text = text
	button.tooltip_text = tooltip if tooltip != "" else text
	button.pressed.connect(pressed)
	bar.add_child(button)
	return button


## A toggle of the toolbar that shows a count, faded when off.
func _toggle(bar: HBoxContainer, icon_name: String, tooltip: String) -> Button:
	var button := Button.new()
	button.flat = true
	button.toggle_mode = true
	button.button_pressed = true
	button.icon = _icon(icon_name)
	button.text = "0"
	button.tooltip_text = tooltip
	var theme := EditorInterface.get_editor_theme()
	var font_color := theme.get_color("font_color", "Button")
	for state in ["font_pressed_color", "font_hover_pressed_color", "font_focus_color"]:
		button.add_theme_color_override(state, font_color)
	for state in ["icon_pressed_color", "icon_hover_pressed_color", "icon_focus_color"]:
		button.add_theme_color_override(state, Color.WHITE)
	button.toggled.connect(
		func(on: bool) -> void:
			button.modulate.a = 1.0 if on else 0.4
			_rebuild()
	)
	bar.add_child(button)
	return button


func _icon(icon_name: String) -> Texture2D:
	var theme := EditorInterface.get_editor_theme()
	if not theme.has_icon(icon_name, "EditorIcons"):
		return null
	return theme.get_icon(icon_name, "EditorIcons")
