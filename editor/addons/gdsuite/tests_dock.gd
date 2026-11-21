@tool
extends VBoxContainer
## The Tests dock: the project's tests, run all, some, or one, and what
## failed. It runs the gdtest gdsuite keeps in the project, with the editor's
## own Godot, and reads gdtest's JSON events as they come.

## A test's result changed: the plugin updates the script editor's gutter.
signal results_changed
## Asks the plugin to open a test: `name` empty opens the script.
signal open_requested(file: String, name: String)
signal watch_toggled(enabled: bool)

const Runner = preload("runner.gd")

const STATUS_IDLE := "idle"
const STATUS_RUNNING := "running"
const STATUS_PASS := "pass"
const STATUS_FAIL := "fail"
const STATUS_ERROR := "error"
const STATUS_SKIP := "skip"

const MENU_RUN := 0
const MENU_OPEN := 1

var runner: Runner
## The folder `gdsuite test` runs, as "res://test/".
var test_folder := "res://test/"

## "res://test/a.gd" -> [test name, ...], in discovery order.
var _tests := {}
## "file::name" -> {status, us, failures}.
var _results := {}
var _pid := -1
var _watch_pid := -1
var _last_failed: PackedStringArray = []
var _counts := {}
var _started_msec := 0

var _tree: Tree
var _details: RichTextLabel
var _status: Label
var _filter: LineEdit
var _run_all: Button
var _run_selected: Button
var _rerun_failed: Button
var _stop: Button
var _watch: CheckButton
var _context_menu: PopupMenu
var _context_target := ""


func _ready() -> void:
	name = "Tests"
	size_flags_vertical = SIZE_EXPAND_FILL
	custom_minimum_size = Vector2(0, 160)

	var bar := HBoxContainer.new()
	add_child(bar)
	_run_all = _button(bar, "Play", "Run all tests", func() -> void: run(PackedStringArray()))
	_run_selected = _button(bar, "PlayStart", "Run the selected tests", _run_selection)
	_rerun_failed = _button(bar, "Reload", "Run the tests that failed again", rerun_failed)
	_stop = _button(bar, "Stop", "Stop the tests", stop)
	bar.add_child(VSeparator.new())
	_watch = CheckButton.new()
	_watch.text = "Rerun on save"
	_watch.tooltip_text = "Run the tests again when a script is saved"
	_watch.toggled.connect(func(on: bool) -> void: watch_toggled.emit(on))
	bar.add_child(_watch)
	_filter = LineEdit.new()
	_filter.placeholder_text = "Filter tests"
	_filter.clear_button_enabled = true
	_filter.custom_minimum_size = Vector2(160, 0)
	_filter.text_changed.connect(func(_text: String) -> void: _rebuild())
	bar.add_child(_filter)
	_status = Label.new()
	_status.size_flags_horizontal = SIZE_EXPAND_FILL
	_status.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_status.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	bar.add_child(_status)

	var split := VSplitContainer.new()
	split.size_flags_vertical = SIZE_EXPAND_FILL
	add_child(split)
	_tree = Tree.new()
	_tree.size_flags_vertical = SIZE_EXPAND_FILL
	_tree.hide_root = true
	_tree.columns = 2
	_tree.select_mode = Tree.SELECT_MULTI
	_tree.set_column_expand(1, false)
	_tree.set_column_custom_minimum_width(1, 90)
	_tree.allow_rmb_select = true
	_tree.item_activated.connect(_on_item_activated)
	_tree.multi_selected.connect(func(_i: TreeItem, _c: int, _s: bool) -> void: _show_details())
	_tree.item_mouse_selected.connect(_on_item_mouse_selected)
	split.add_child(_tree)
	_details = RichTextLabel.new()
	_details.custom_minimum_size = Vector2(0, 60)
	_details.selection_enabled = true
	_details.bbcode_enabled = false
	split.add_child(_details)

	_context_menu = PopupMenu.new()
	_context_menu.id_pressed.connect(_on_context_menu)
	add_child(_context_menu)

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
## test when empty.
func run(targets: PackedStringArray) -> void:
	var gdtest := Runner.gdtest_program()
	if gdtest == "" or is_running():
		return
	if targets.is_empty():
		targets = PackedStringArray([test_folder])
	_counts = {}
	_started_msec = Time.get_ticks_msec()
	_details.text = ""
	_mark_running(targets)
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
	runner.kill(_pid)


func is_running() -> bool:
	return _pid > 0


## Keeps gdtest's watch mode running while `enabled`: it reruns the tests
## when scripts change.
func set_watch(enabled: bool) -> void:
	_watch.set_pressed_no_signal(enabled)
	if not enabled:
		runner.kill(_watch_pid)
		_watch_pid = -1
		return
	var gdtest := Runner.gdtest_program()
	if gdtest == "" or _watch_pid > 0:
		return
	var args := _gdtest_args(PackedStringArray(["--watch"]), PackedStringArray([test_folder]))
	_watch_pid = runner.stream(gdtest, args, _on_event, _on_watch_done)


## The tests of `file` and their status: [{name, status}].
func tests_of(file: String) -> Array:
	var tests := []
	for test_name: String in _tests.get(file, []):
		var result: Dictionary = _results.get(_key(file, test_name), {})
		tests.append({ "name": test_name, "status": result.get("status", STATUS_IDLE) })
	return tests


# --- Running ------------------------------------------------------------------


func _on_listed(line: String, found: Dictionary) -> void:
	var event := _event(line)
	if event.get("kind") != "test":
		return
	if not found.has(event.file):
		found[event.file] = []
	found[event.file].append(event.name)


func _on_list_done(_code: int, found: Dictionary) -> void:
	_tests = found
	_rebuild()
	_update_status()


func _on_watch_done(_code: int) -> void:
	_watch_pid = -1
	_watch.set_pressed_no_signal(false)


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
	# gdtest reads paths from the project's folder; it doesn't walk a
	# "res://" folder. One test is chosen with --filter, which keeps the JSON
	# output: a "file::test" target doesn't.
	var paths := PackedStringArray()
	var patterns := PackedStringArray()
	var one_test := false
	for target in targets:
		var file := target.get_slice("::", 0)
		var path := file.trim_prefix("res://")
		if not path in paths:
			paths.append(path if path != "" else ".")
		var pattern := _regex_escape(file)
		if file.ends_with("/"):
			pattern += ".*"
		pattern += " :: "
		if "::" in target:
			one_test = true
			pattern += _regex_escape(target.get_slice("::", 1)) + "$"
		patterns.append(pattern)
	if one_test:
		args.append_array(PackedStringArray(["--filter", "^(%s)" % "|".join(patterns)]))
	args.append_array(paths)
	return args


static func _regex_escape(text: String) -> String:
	var escaped := ""
	for character in text:
		if character in "\\.^$|?*+()[]{}":
			escaped += "\\"
		escaped += character
	return escaped


func _on_event(line: String) -> void:
	var event := _event(line)
	var kind: String = event.get("kind", "")
	if kind == "summary":
		_counts = event
		_update_status()
		return
	var file: String = event.get("file", "")
	var test_name: String = event.get("name", "")
	if file == "" or test_name == "":
		return
	if not _tests.has(file):
		_tests[file] = []
	if not test_name in _tests[file]:
		_tests[file].append(test_name)
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
	var failures: Array = event.get("failures", [])
	if kind == "test_error":
		failures = [str(event.get("message", "the test could not run"))]
	_results[_key(file, test_name)] = {
		"status": status,
		"us": int(event.get("us", 0)),
		"failures": failures,
	}
	_rebuild()
	if status != STATUS_RUNNING:
		results_changed.emit()


func _on_run_done(_code: int) -> void:
	_pid = -1
	_set_running(false)
	# Tests still marked running were stopped, or their worker crashed.
	for key: String in _results:
		if _results[key].status == STATUS_RUNNING:
			_results[key].status = STATUS_IDLE
	_last_failed = PackedStringArray()
	for key: String in _results:
		if _results[key].status in [STATUS_FAIL, STATUS_ERROR]:
			_last_failed.append(key)
	_rerun_failed.disabled = _last_failed.is_empty()
	_rebuild()
	_update_status()
	_show_details()
	results_changed.emit()


func _mark_running(targets: PackedStringArray) -> void:
	for target in targets:
		var file := target.get_slice("::", 0)
		var only := target.get_slice("::", 1) if "::" in target else ""
		for test_file: String in _tests:
			var in_target := test_file == file or test_file.begins_with(file.trim_suffix("/") + "/")
			if not in_target:
				continue
			for test_name: String in _tests[test_file]:
				if only == "" or only == test_name:
					var running := { "status": STATUS_RUNNING, "us": 0, "failures": [] }
					_results[_key(test_file, test_name)] = running
	_rebuild()


func _run_selection() -> void:
	var targets := PackedStringArray()
	var item := _tree.get_next_selected(null)
	while item != null:
		var meta: Dictionary = item.get_metadata(0)
		targets.append(meta.file if meta.name == "" else _key(meta.file, meta.name))
		item = _tree.get_next_selected(item)
	if not targets.is_empty():
		run(targets)


# --- Showing ------------------------------------------------------------------


func _rebuild() -> void:
	_tree.clear()
	var root := _tree.create_item()
	var needle := _filter.text.strip_edges().to_lower()
	var files := _tests.keys()
	files.sort()
	for file: String in files:
		var names: Array = _tests[file].filter(
			func(n: String) -> bool:
				return needle == "" or needle in n.to_lower() or needle in file.to_lower()
		)
		if names.is_empty():
			continue
		var file_item := _tree.create_item(root)
		file_item.set_text(0, file.trim_prefix("res://"))
		file_item.set_metadata(0, { "file": file, "name": "" })
		var worst := STATUS_IDLE
		var total_us := 0
		for test_name: String in names:
			var result: Dictionary = _results.get(_key(file, test_name), {})
			var status: String = result.get("status", STATUS_IDLE)
			worst = _worse(worst, status)
			total_us += int(result.get("us", 0))
			var item := _tree.create_item(file_item)
			item.set_text(0, test_name.trim_prefix("test_").replace("_", " "))
			item.set_tooltip_text(0, test_name)
			item.set_metadata(0, { "file": file, "name": test_name })
			item.set_icon(0, _status_icon(status))
			item.set_text(1, _duration(int(result.get("us", 0))))
			item.set_text_alignment(1, HORIZONTAL_ALIGNMENT_RIGHT)
		file_item.set_icon(0, _status_icon(worst) if worst != STATUS_IDLE else _icon("GDScript"))
		file_item.set_text(1, _duration(total_us))
		file_item.set_text_alignment(1, HORIZONTAL_ALIGNMENT_RIGHT)
		file_item.collapsed = worst in [STATUS_IDLE, STATUS_PASS] and needle == ""


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
		_status.text = "Running..."
		return
	var total := 0
	for file: String in _tests:
		total += _tests[file].size()
	if _counts.is_empty():
		_status.text = "%d tests" % total
		return
	var failed := int(_counts.get("fail", 0)) + int(_counts.get("error", 0))
	var seconds := (Time.get_ticks_msec() - _started_msec) / 1000.0
	_status.text = "%d passed, %d failed, %d skipped (%.1f s)" % [
		int(_counts.get("pass", 0)),
		failed,
		int(_counts.get("skipped", 0)) + int(_counts.get("pending", 0)),
		seconds,
	]


func _set_running(running: bool) -> void:
	_run_all.disabled = running
	_run_selected.disabled = running
	_rerun_failed.disabled = running or _last_failed.is_empty()
	_stop.disabled = not running
	_update_status()


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
	_context_menu.add_item("Run", MENU_RUN)
	_context_menu.add_item("Open", MENU_OPEN)
	_context_menu.position = Vector2i(get_screen_position() + get_local_mouse_position())
	_context_menu.reset_size()
	_context_menu.popup()


func _on_context_menu(id: int) -> void:
	match id:
		MENU_RUN:
			run(PackedStringArray([_context_target]))
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
	var data: Variant = JSON.parse_string(trimmed)
	return data if data is Dictionary else {}


func _worse(a: String, b: String) -> String:
	var order := [STATUS_IDLE, STATUS_SKIP, STATUS_PASS, STATUS_RUNNING, STATUS_FAIL, STATUS_ERROR]
	return a if order.find(a) >= order.find(b) else b


func _status_icon(status: String) -> Texture2D:
	match status:
		STATUS_PASS:
			return _icon("StatusSuccess")
		STATUS_FAIL, STATUS_ERROR:
			return _icon("StatusError")
		STATUS_SKIP:
			return _icon("StatusWarning")
		STATUS_RUNNING:
			return _icon("Reload")
	return null


func _duration(us: int) -> String:
	if us <= 0:
		return ""
	if us < 1000:
		return "%d µs" % us
	if us < 1000000:
		return "%.1f ms" % (us / 1000.0)
	return "%.2f s" % (us / 1000000.0)


func _button(bar: HBoxContainer, icon_name: String, tooltip: String, pressed: Callable) -> Button:
	var button := Button.new()
	button.flat = true
	button.icon = _icon(icon_name)
	button.tooltip_text = tooltip
	button.pressed.connect(pressed)
	bar.add_child(button)
	return button


func _icon(icon_name: String) -> Texture2D:
	return EditorInterface.get_editor_theme().get_icon(icon_name, "EditorIcons")
