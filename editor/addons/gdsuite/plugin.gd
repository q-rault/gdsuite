@tool
extends EditorPlugin
## GDSuite in the Godot editor: a Check button, a Problems dock, problems
## marked in the script editor, formatting, and a Tests dock. Everything runs the gdsuite
## program, with the project's gdsuite.toml, so the editor agrees with the
## terminal and CI.

const Runner = preload("runner.gd")
const Settings = preload("settings.gd")
const Markers = preload("markers.gd")
const ProblemsDock = preload("problems_dock.gd")
const TestsDock = preload("tests_dock.gd")
const TestsGutter = preload("tests_gutter.gd")
const TestsMenu = preload("tests_menu.gd")

const DOCS := "https://q-rault.github.io/gdsuite"
const NOT_FOUND := "gdsuite not found: install it, or set its path in Editor Settings > GDSuite"
const INSTALL_DOCS := DOCS + "/install"
const RULE_DOCS := DOCS + "/reference/lint-rules#"
## Covers its own line and the next, as gdtidy reads it.
const IGNORE_COMMENT := "  # gdtidy: ignore=%s"

## The toolbar's icons, drawn white and coloured by the state they show.
const CHECK_ICON := (
	'<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">'
	+ '<path d="M7.5 14.5H3v-13h6.5L13 5v3" fill="none" stroke="#fff" stroke-width="1.5" '
	+ 'stroke-linejoin="round"/><path d="M8.5 11.5l2.2 2.2 4-4.4" fill="none" stroke="#fff" '
	+ 'stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>'
)
const TESTS_ICON := (
	'<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">'
	+ '<path d="M5.5 1.5h5M6.5 1.5V6l-4 7.2a.9.9 0 0 0 .8 1.3h9.4a.9.9 0 0 0 .8-1.3L9.5 6V1.5" '
	+ 'fill="none" stroke="#fff" stroke-width="1.5" stroke-linecap="round" '
	+ 'stroke-linejoin="round"/><path d="M4.4 10h7.2l1.5 3H2.9z" fill="#fff"/></svg>'
)
const STATE_NONE := "none"
const STATE_OK := "ok"
const STATE_WARNING := "warning"
const STATE_ERROR := "error"

const SHORTCUT_FORMAT := "gdsuite/format_script"
const COMMAND_FORMAT := "gdsuite/format_script"
const COMMAND_CHECK := "gdsuite/check"
const COMMAND_PROBLEMS := "gdsuite/show_problems"
const COMMAND_REFRESH := "gdsuite/refresh_problems"
const COMMAND_RUN_TESTS := "gdsuite/run_all_tests"
const COMMAND_RERUN_FAILED := "gdsuite/rerun_failed_tests"
const COMMAND_RUN_SCRIPT_TESTS := "gdsuite/run_script_tests"
const COMMAND_SHOW_TESTS := "gdsuite/show_tests"
const COMMANDS := [
	COMMAND_FORMAT,
	COMMAND_CHECK,
	COMMAND_PROBLEMS,
	COMMAND_REFRESH,
	COMMAND_RUN_TESTS,
	COMMAND_RERUN_FAILED,
	COMMAND_RUN_SCRIPT_TESTS,
	COMMAND_SHOW_TESTS,
]

const MENU_FORMAT := 0
const MENU_CHECK := 1
const MENU_PROBLEMS := 2
const MENU_REFRESH := 3
const MENU_CONFIG := 4
const MENU_RUN_TESTS := 5
const MENU_TESTS := 6

## EditorDock.DockSlot.SLOT_BOTTOM and DockLayout flags, written as
## numbers so the addon also loads on Godot 4.5, which has no EditorDock.
const SLOT_BOTTOM := 8
const LAYOUT_HORIZONTAL := 2
const LAYOUT_FLOATING := 4

var _runner := Runner.new()
var _markers := Markers.new()
var _dock: ProblemsDock
## The EditorDock holding `_dock` on Godot 4.6 and newer; null on 4.5.
var _dock_host: Control
var _tests: TestsDock
var _tests_host: Control
var _tests_gutter := TestsGutter.new()
var _tests_menu: TestsMenu
## Waits for the file system to settle before finding the tests again.
var _discover_timer: Timer
var _check_button: Button
var _tests_button: Button
var _check_running := false
## The steps of the last check, for the Check icon's tooltip.
var _check_lines := PackedStringArray()
var _menu: PopupMenu
var _format_shortcut: Shortcut
var _type_check_timer: Timer
var _type_check_running := false
var _type_check_again := false
## Set while this addon saves a script, so its own save starts nothing.
var _saving := false
var _connected_code_edits := {}


func _get_plugin_name() -> String:
	return "GDSuite"


func _enter_tree() -> void:
	Settings.register()

	_dock = ProblemsDock.new()
	_dock.action_requested.connect(_on_problem_action)
	_dock.refresh_requested.connect(_refresh_project)
	_dock.problems_changed.connect(_show_markers)
	_dock.setting_toggled.connect(_on_setting_toggled)
	_dock.open_config_requested.connect(_open_config)
	_dock_host = _add_dock(_dock, "Problems", "gdsuite_problems")

	_tests = TestsDock.new()
	_tests.runner = _runner
	_tests.results_changed.connect(_show_test_marks)
	_tests.open_requested.connect(_open_test)
	_tests.watch_toggled.connect(_on_watch_toggled)
	_tests.results_changed.connect(_show_tests_state)
	_tests_host = _add_dock(_tests, "Tests", "gdsuite_tests")
	_tests_menu = TestsMenu.new()
	_tests_menu.run_paths = _run_tests_and_show
	add_context_menu_plugin(EditorContextMenuPlugin.CONTEXT_SLOT_FILESYSTEM, _tests_menu)

	_check_button = _status_button(CHECK_ICON, _run_check)
	_tests_button = _status_button(TESTS_ICON, _on_tests_button)
	add_control_to_container(CONTAINER_TOOLBAR, _check_button)
	add_control_to_container(CONTAINER_TOOLBAR, _tests_button)
	_dock.problems_changed.connect(_show_check_state)

	_menu = PopupMenu.new()
	_menu.add_item("Format script", MENU_FORMAT)
	_menu.add_item("Check the project", MENU_CHECK)
	_menu.add_item("Show problems", MENU_PROBLEMS)
	_menu.add_item("Lint and type check the project", MENU_REFRESH)
	_menu.add_separator()
	_menu.add_item("Run all tests", MENU_RUN_TESTS)
	_menu.add_item("Show tests", MENU_TESTS)
	_menu.add_separator()
	_menu.add_item("Open gdsuite.toml", MENU_CONFIG)
	_menu.id_pressed.connect(_on_menu)
	add_tool_submenu_item("GDSuite", _menu)

	_format_shortcut = _register_shortcut()
	var palette := EditorInterface.get_command_palette()
	var format_keys := _format_shortcut.get_as_text()
	palette.add_command("Format Script", COMMAND_FORMAT, _format_current_script, format_keys)
	palette.add_command("Check the Project", COMMAND_CHECK, _run_check)
	palette.add_command("Show Problems", COMMAND_PROBLEMS, _show_dock)
	palette.add_command("Lint and Type Check the Project", COMMAND_REFRESH, _refresh_project)
	var run_all := _run_tests_and_show.bind(PackedStringArray())
	palette.add_command("Run All Tests", COMMAND_RUN_TESTS, run_all)
	palette.add_command("Rerun Failed Tests", COMMAND_RERUN_FAILED, _tests.rerun_failed)
	palette.add_command("Run Tests in Script", COMMAND_RUN_SCRIPT_TESTS, _run_script_tests)
	palette.add_command("Show Tests", COMMAND_SHOW_TESTS, _show_tests)

	_type_check_timer = Timer.new()
	_type_check_timer.one_shot = true
	_type_check_timer.wait_time = 1.5
	_type_check_timer.timeout.connect(_type_check)
	add_child(_type_check_timer)

	_discover_timer = Timer.new()
	_discover_timer.one_shot = true
	_discover_timer.wait_time = 1.0
	_discover_timer.timeout.connect(_tests.discover)
	add_child(_discover_timer)

	resource_saved.connect(_on_resource_saved)
	scene_saved.connect(_on_scene_saved)
	var script_editor := EditorInterface.get_script_editor()
	script_editor.editor_script_changed.connect(_on_script_changed)
	EditorInterface.get_resource_filesystem().filesystem_changed.connect(_on_filesystem_changed)

	if _runner.find_program() == "":
		_dock.set_status(NOT_FOUND)
		_paint(_check_button, STATE_NONE, "gdsuite not found. Install GDSuite: " + INSTALL_DOCS)
		_paint(_tests_button, STATE_NONE, "gdsuite not found. Install GDSuite: " + INSTALL_DOCS)
		return
	_read_project_settings()
	_show_check_state()
	_show_tests_state()
	_refresh_project()


func _exit_tree() -> void:
	resource_saved.disconnect(_on_resource_saved)
	scene_saved.disconnect(_on_scene_saved)
	var script_editor := EditorInterface.get_script_editor()
	if script_editor.editor_script_changed.is_connected(_on_script_changed):
		script_editor.editor_script_changed.disconnect(_on_script_changed)
	var filesystem := EditorInterface.get_resource_filesystem()
	if filesystem.filesystem_changed.is_connected(_on_filesystem_changed):
		filesystem.filesystem_changed.disconnect(_on_filesystem_changed)
	for id: int in _connected_code_edits:
		var code_edit := instance_from_id(id) as CodeEdit
		if is_instance_valid(code_edit):
			_markers.clear(code_edit)
			_tests_gutter.clear(code_edit)
			code_edit.gutter_clicked.disconnect(_connected_code_edits[id])
	_connected_code_edits.clear()

	var palette := EditorInterface.get_command_palette()
	for command: String in COMMANDS:
		palette.remove_command(command)
	remove_tool_menu_item("GDSuite")
	remove_context_menu_plugin(_tests_menu)
	remove_control_from_container(CONTAINER_TOOLBAR, _check_button)
	remove_control_from_container(CONTAINER_TOOLBAR, _tests_button)
	_check_button.queue_free()
	_tests_button.queue_free()
	_remove_dock(_dock, _dock_host)
	_remove_dock(_tests, _tests_host)
	_runner.stop()


func _shortcut_input(event: InputEvent) -> void:
	if not event.is_pressed() or event.is_echo():
		return
	var in_scripts := EditorInterface.get_script_editor().is_visible_in_tree()
	if in_scripts and _format_shortcut.matches_event(event):
		_format_current_script()
		get_viewport().set_input_as_handled()


# --- Check ------------------------------------------------------------------


func _run_check() -> void:
	if _runner.program == "":
		_toast("gdsuite not found. Install GDSuite: " + INSTALL_DOCS, true)
		return
	if _check_running:
		return
	_check_running = true
	_show_check_state()
	_runner.run(PackedStringArray(["check", "--report", "json"]), _on_check_done)


func _on_check_done(result: Dictionary) -> void:
	_check_running = false
	var report: Variant = result.json
	if not report is Dictionary:
		var output := str(result.output).strip_edges()
		_check_lines = PackedStringArray(["The check could not run:", output])
		_show_check_state()
		return
	_check_lines = PackedStringArray()
	for step: Dictionary in report.get("steps", []):
		var mark := "✔" if step.get("status") == "ok" else "✖"
		_check_lines.append("%s %s: %s" % [mark, step.get("name", ""), step.get("summary", "")])
	_refresh_project()
	if report.get("status") != "ok":
		_show_dock()


## The Check icon: grey until format, lint and the type check have all run,
## then red for errors, yellow for warnings, green for none.
func _show_check_state() -> void:
	var lines := PackedStringArray()
	var state := STATE_NONE
	if _check_running:
		lines.append("Checking...")
	elif not _dock.is_known():
		lines.append("Looking at the project...")
	else:
		var count := _dock.counts()
		if count.errors > 0:
			state = STATE_ERROR
		elif count.warnings > 0:
			state = STATE_WARNING
		else:
			state = STATE_OK
		var counted := "%d errors, %d warnings" % [count.errors, count.warnings]
		lines.append(counted + " (format, lint, types)")
	lines.append("Click to run the project's check: the steps of [check] in gdsuite.toml")
	if not _check_lines.is_empty():
		lines.append("")
		lines.append("Last check:")
		lines.append_array(_check_lines)
	_paint(_check_button, state, "\n".join(lines))


## The Tests icon: grey while some tests have not run since they changed,
## red when one fails, green when all ran and pass.
func _show_tests_state() -> void:
	var state := STATE_NONE
	match _tests.state():
		TestsDock.STATE_FAIL:
			state = STATE_ERROR
		TestsDock.STATE_PASS:
			state = STATE_OK
	_paint(_tests_button, state, _tests.summary() + "\nClick to run all tests")


func _on_tests_button() -> void:
	if _tests.is_running():
		_show_tests()
	else:
		_run_tests_and_show(PackedStringArray())


func _status_button(svg: String, pressed: Callable) -> Button:
	var button := Button.new()
	button.flat = true
	button.icon = _svg_icon(svg)
	button.pressed.connect(pressed)
	return button


func _paint(button: Button, state: String, tooltip: String) -> void:
	var theme := EditorInterface.get_editor_theme()
	var color := Color(0.6, 0.6, 0.6)
	match state:
		STATE_OK:
			color = theme.get_color("success_color", "Editor")
		STATE_WARNING:
			color = Color(1.0, 0.8, 0.25)
		STATE_ERROR:
			color = theme.get_color("error_color", "Editor")
	for color_name: String in [
		"icon_normal_color",
		"icon_hover_color",
		"icon_pressed_color",
		"icon_hover_pressed_color",
		"icon_focus_color",
	]:
		button.add_theme_color_override(color_name, color)
	button.tooltip_text = tooltip


func _svg_icon(svg: String) -> Texture2D:
	var image := Image.new()
	image.load_svg_from_string(svg, EditorInterface.get_editor_scale())
	return ImageTexture.create_from_image(image)


# --- Problems ---------------------------------------------------------------


func _refresh_project() -> void:
	_format_check(PackedStringArray())
	_lint(PackedStringArray())
	_type_check()


## Finds the scripts of `paths` ("res://" paths), or of the project, that
## formatting would change.
func _format_check(paths: PackedStringArray) -> void:
	if _runner.program == "":
		return
	var args := PackedStringArray(["fmt", "--", "--check", "--format", "json"])
	if paths.is_empty():
		args.append(".")
	for path in paths:
		args.append(path.trim_prefix("res://"))
	_runner.run(args, _on_format_check_done.bind(paths))


func _on_format_check_done(result: Dictionary, paths: PackedStringArray) -> void:
	if not result.json is Dictionary:
		_dock.set_status("The format check could not run: " + str(result.output).strip_edges())
		return
	var problems := []
	for path: Variant in result.json.get("changed", []):
		problems.append({
			"file": "res://" + str(path).replace("\\", "/").trim_prefix("./"),
			"line": 1,
			"column": 1,
			"severity": "warning",
			"message": "Not formatted: Shift+Alt+F formats it",
			"rule": "format",
			"fixable": true,
		})
	_dock.set_format(paths, problems)


## Lints `paths` ("res://" paths), or the whole project when it is empty.
func _lint(paths: PackedStringArray) -> void:
	if _runner.program == "":
		return
	var args := PackedStringArray(["lint", "--report", "json", "--"])
	if paths.is_empty():
		args.append(".")
	for path in paths:
		args.append(path.trim_prefix("res://"))
	_runner.run(args, _on_lint_done.bind(paths))


func _on_lint_done(result: Dictionary, paths: PackedStringArray) -> void:
	if not result.json is Dictionary:
		_dock.set_status("Lint could not run: " + str(result.output).strip_edges())
		return
	_dock.set_lint(paths, _problems_from(result.json))


func _type_check() -> void:
	if _runner.program == "":
		return
	if _type_check_running:
		_type_check_again = true
		return
	_type_check_running = true
	_dock.set_status("Type checking...")
	_runner.run(PackedStringArray(["typecheck", "--report", "json"]), _on_type_check_done)


func _on_type_check_done(result: Dictionary) -> void:
	_type_check_running = false
	if result.json is Dictionary:
		_dock.set_types(_problems_from(result.json))
	else:
		_dock.set_status("The type check could not run: " + str(result.output).strip_edges())
	if _type_check_again:
		_type_check_again = false
		_type_check()


## The findings of a `--report json` result, as the dock's problems.
func _problems_from(report: Dictionary) -> Array:
	var problems := []
	for finding: Dictionary in report.get("findings", []):
		problems.append({
			"file": "res://" + str(finding.get("file", "")).replace("\\", "/").trim_prefix("./"),
			"line": int(finding.get("line", 1)),
			"column": int(finding.get("column", 1)),
			"severity": str(finding.get("severity", "warning")),
			"message": str(finding.get("message", "")),
			"rule": str(finding.get("rule", "")),
			"fixable": bool(finding.get("fixable", false)),
		})
	return problems


func _on_problem_action(action: String, problem: Dictionary) -> void:
	match action:
		"open":
			_open(problem)
		"fix":
			_fix(problem)
		"ignore_here":
			_ignore_here(problem)
		"disable_rule":
			_runner.run(
				PackedStringArray(["config", "add", "lint.disable", problem.rule]),
				func(result: Dictionary) -> void:
					if result.code != 0:
						_toast("Could not change gdsuite.toml: " + str(result.output), true)
						return
					_toast('"%s" is turned off in gdsuite.toml' % problem.rule)
					_lint(PackedStringArray())
			)
		"docs":
			OS.shell_open(RULE_DOCS + problem.rule)


func _open(problem: Dictionary) -> void:
	var script := load(problem.file) as Script
	if script == null:
		return
	EditorInterface.edit_script(script, int(problem.line), int(problem.get("column", 1)) - 1)


## Fixes the script on disk (formats it, for a format problem), then shows
## the result in the open editor. A script with unsaved changes is left
## alone: they would be lost.
func _fix(problem: Dictionary) -> void:
	var code_edit := _code_edit_for(problem.file)
	if code_edit != null and code_edit.get_version() != code_edit.get_saved_version():
		_toast("Save the script first: fixing works on the saved file", true)
		return
	var file: String = problem.file.trim_prefix("res://")
	var args := PackedStringArray(["lint", "--", "--fix", file])
	if problem.get("source") == "format":
		args = PackedStringArray(["fmt", "--", file])
	_runner.run(
		args,
		func(_result: Dictionary) -> void:
			var text := FileAccess.get_file_as_string(problem.file)
			var script := load(problem.file) as GDScript
			if script != null:
				script.source_code = text
			if code_edit != null and is_instance_valid(code_edit) and text != "":
				_replace_text(code_edit, text, true)
			_format_check(PackedStringArray([problem.file]))
			_lint(PackedStringArray([problem.file]))
	)


func _ignore_here(problem: Dictionary) -> void:
	_open(problem)
	var code_edit := _code_edit_for(problem.file)
	if code_edit == null:
		return
	var line := int(problem.line) - 1
	code_edit.begin_complex_operation()
	code_edit.set_line(line, code_edit.get_line(line) + IGNORE_COMMENT % problem.rule)
	code_edit.end_complex_operation()


func _show_markers() -> void:
	var path := _current_script_path()
	var code_edit := _current_code_edit()
	if code_edit == null:
		return
	if not Settings.value(Settings.SHOW_IN_GUTTER):
		_markers.clear(code_edit)
	else:
		# A script that is not formatted has no line to mark.
		var problems := _dock.problems_for(path).filter(
			func(p: Dictionary) -> bool: return p.source != "format"
		)
		_markers.show(code_edit, problems)


func _on_script_changed(_script: Script) -> void:
	_dock.set_current_script(_current_script_path())
	var code_edit := _current_code_edit()
	if code_edit != null and not _connected_code_edits.has(code_edit.get_instance_id()):
		var on_click := _on_gutter_clicked.bind(code_edit)
		code_edit.gutter_clicked.connect(on_click)
		_connected_code_edits[code_edit.get_instance_id()] = on_click
	_show_markers()
	_show_test_marks()


func _on_gutter_clicked(line: int, gutter: int, code_edit: CodeEdit) -> void:
	var test_name := _tests_gutter.test_at(code_edit, line, gutter)
	if test_name != "":
		_run_tests_and_show(PackedStringArray(["%s::%s" % [_current_script_path(), test_name]]))
		return
	var problems := _markers.problems_at(code_edit, line, gutter)
	if problems.is_empty():
		return
	var messages := PackedStringArray()
	for problem: Dictionary in problems:
		messages.append("%s (%s)" % [problem.message, problem.rule])
	var has_error := problems.any(func(p: Dictionary) -> bool: return p.severity == "error")
	_toast("\n".join(messages), has_error)


# --- Tests ------------------------------------------------------------------


func _run_tests_and_show(targets: PackedStringArray) -> void:
	if Runner.gdtest_program() == "":
		_toast("Run `gdsuite test` once in a terminal to set up the tests", true)
		return
	_tests.run(targets)
	_show_tests()


func _run_script_tests() -> void:
	var path := _current_script_path()
	if path == "" or _tests.tests_of(path).is_empty():
		_toast("The current script has no tests")
		return
	_run_tests_and_show(PackedStringArray([path]))


## The test marks in the gutter of the script being edited.
func _show_test_marks() -> void:
	var code_edit := _current_code_edit()
	if code_edit == null:
		return
	var tests := _tests.tests_of(_current_script_path())
	if tests.is_empty():
		_tests_gutter.clear(code_edit)
	else:
		_tests_gutter.show(code_edit, tests)


func _open_test(file: String, test_name: String) -> void:
	var script := load(file) as Script
	if script == null:
		return
	EditorInterface.edit_script(script)
	if test_name == "":
		return
	var code_edit := _code_edit_for(file)
	var line := -1 if code_edit == null else TestsGutter.line_of(code_edit, test_name)
	if line >= 0:
		EditorInterface.edit_script(script, line + 1)


func _on_watch_toggled(enabled: bool) -> void:
	Settings.set_personal(Settings.RUN_TESTS_ON_SAVE, enabled)
	_tests.set_watch(Settings.value(Settings.RUN_TESTS_ON_SAVE))


func _on_filesystem_changed() -> void:
	_read_project_settings()
	_discover_timer.start()


# --- Saving and formatting --------------------------------------------------


func _on_resource_saved(resource: Resource) -> void:
	if _saving or _runner.program == "":
		return
	var path := resource.resource_path
	# Built-in scripts and resources are saved with their scene.
	if path == "" or path.contains("::"):
		return
	if not resource is GDScript:
		_tests.file_saved(path)
		return
	if Settings.value(Settings.FORMAT_ON_SAVE):
		_format_saved(resource as GDScript)
	_format_check(PackedStringArray([path]))
	_tests.file_saved(path)
	if Settings.value(Settings.LINT_ON_SAVE):
		_lint(PackedStringArray([path]))
	if Settings.value(Settings.TYPE_CHECK_ON_SAVE):
		_type_check_timer.start()


func _on_scene_saved(path: String) -> void:
	if not _saving:
		_tests.file_saved(path)


func _format_current_script() -> void:
	var script := EditorInterface.get_script_editor().get_current_script()
	var code_edit := _current_code_edit()
	if not script is GDScript or code_edit == null:
		_toast("Open a GDScript file to format it")
		return
	var formatted: Variant = _format_text(code_edit.text)
	if formatted != null and formatted != code_edit.text:
		_replace_text(code_edit, formatted, false)


## Formats a saved script, saves it again, and shows it in its open editor.
func _format_saved(script: GDScript) -> void:
	var formatted: Variant = _format_text(script.source_code)
	if formatted == null or formatted == script.source_code:
		return
	script.source_code = formatted
	_saving = true
	ResourceSaver.save(script)
	_saving = false
	var code_edit := _code_edit_for(script.resource_path)
	if code_edit != null:
		_replace_text(code_edit, formatted, true)


## `text` formatted with the project's settings, or null when gdsuite leaves
## it as it is.
func _format_text(text: String) -> Variant:
	if _runner.program == "":
		_toast("gdsuite not found. Install GDSuite: " + INSTALL_DOCS, true)
		return null
	var input := OS.get_temp_dir().path_join("gdsuite-format-%d.gd" % OS.get_process_id())
	var file := FileAccess.open(input, FileAccess.WRITE)
	if file == null:
		return null
	file.store_string(text)
	file.close()
	var result := _runner.run_now(PackedStringArray(["fmt", "--input", input]))
	DirAccess.remove_absolute(input)
	if result.code != 0:
		_toast(
			"The script was left as it is: it has a syntax error, "
			+ "or formatting would change what it does",
			true,
		)
		return null
	# Never replace a script with nothing.
	if str(result.output).strip_edges() == "" and text.strip_edges() != "":
		_toast("The script was left as it is: gdsuite returned no text", true)
		return null
	return result.output


## Replaces the editor's text in one undo step, keeping the caret and the
## scroll position. `saved` marks the result as saved.
func _replace_text(code_edit: CodeEdit, text: String, saved: bool) -> void:
	var line := code_edit.get_caret_line()
	var column := code_edit.get_caret_column()
	var scroll := code_edit.scroll_vertical
	code_edit.begin_complex_operation()
	code_edit.select_all()
	code_edit.delete_selection()
	code_edit.insert_text_at_caret(text)
	code_edit.end_complex_operation()
	code_edit.set_caret_line(mini(line, code_edit.get_line_count() - 1))
	code_edit.set_caret_column(column)
	code_edit.scroll_vertical = scroll
	if saved:
		code_edit.tag_saved_version()
	code_edit.text_changed.emit()


# --- Settings ---------------------------------------------------------------


func _read_project_settings() -> void:
	if _runner.program == "":
		return
	_runner.run(
		PackedStringArray(["config", "get", "editor"]),
		func(result: Dictionary) -> void:
			Settings.project_values = result.json if result.json is Dictionary else {}
			_tests.set_watch(Settings.value(Settings.RUN_TESTS_ON_SAVE))
	)
	var key := "commands.default_test_target"
	_runner.run(PackedStringArray(["config", "get", key]), _on_test_folder)


## Reads the folder `gdsuite test` runs, then finds its tests.
func _on_test_folder(result: Dictionary) -> void:
	# JSON.new().parse, unlike JSON.parse_string, prints nothing on bad input.
	var json := JSON.new()
	var folder: Variant = null
	if result.code == 0 and json.parse(str(result.output).strip_edges()) == OK:
		folder = json.data
	if not folder is String or folder == "":
		folder = "test/"
	if not folder.begins_with("res://"):
		folder = "res://" + folder.trim_prefix("./")
	_tests.test_folder = folder
	_tests.discover()


func _on_setting_toggled(key: String, _enabled: bool) -> void:
	if key == Settings.SHOW_IN_GUTTER:
		_show_markers()
	elif key == Settings.RUN_TESTS_ON_SAVE:
		_tests.set_watch(Settings.value(key))


func _open_config() -> void:
	var path := Runner.project_root().path_join("gdsuite.toml")
	if not FileAccess.file_exists(path):
		_toast(
			"This project has no gdsuite.toml: GDSuite uses its defaults. "
			+ "Run `gdsuite init` to write one."
		)
		return
	OS.shell_open(path)


func _register_shortcut() -> Shortcut:
	var settings := EditorInterface.get_editor_settings()
	if settings.has_method("get_shortcut") and settings.call("has_shortcut", SHORTCUT_FORMAT):
		return settings.call("get_shortcut", SHORTCUT_FORMAT)
	var key := InputEventKey.new()
	key.keycode = KEY_F
	key.shift_pressed = true
	key.alt_pressed = true
	var shortcut := Shortcut.new()
	shortcut.events = [key]
	# Godot 4.6 and newer list it in Editor Settings > Shortcuts, to rebind.
	if settings.has_method("add_shortcut"):
		settings.call("add_shortcut", SHORTCUT_FORMAT, shortcut)
	return shortcut


# --- Helpers ----------------------------------------------------------------


func _on_menu(id: int) -> void:
	match id:
		MENU_FORMAT:
			_format_current_script()
		MENU_CHECK:
			_run_check()
		MENU_PROBLEMS:
			_show_dock()
		MENU_REFRESH:
			_refresh_project()
		MENU_CONFIG:
			_open_config()
		MENU_RUN_TESTS:
			_run_tests_and_show(PackedStringArray())
		MENU_TESTS:
			_show_tests()


func _show_dock() -> void:
	_show_in_dock(_dock, _dock_host)


func _show_tests() -> void:
	_show_in_dock(_tests, _tests_host)


## Adds `control` as a dock of its own: an EditorDock on Godot 4.6 and newer,
## which it returns, or a bottom panel on 4.5, returning null.
func _add_dock(control: Control, title: String, layout_key: String) -> Control:
	if not ClassDB.class_exists("EditorDock"):
		add_control_to_bottom_panel(control, title)
		return null
	var host: Control = ClassDB.instantiate("EditorDock")
	host.set("title", title)
	host.set("layout_key", layout_key)
	host.set("default_slot", SLOT_BOTTOM)
	host.set("available_layouts", LAYOUT_HORIZONTAL | LAYOUT_FLOATING)
	host.add_child(control)
	call("add_dock", host)
	return host


func _remove_dock(control: Control, host: Control) -> void:
	if host != null:
		call("remove_dock", host)
		host.queue_free()
	else:
		remove_control_from_bottom_panel(control)
		control.queue_free()


func _show_in_dock(control: Control, host: Control) -> void:
	if host != null:
		host.call("make_visible")
	else:
		make_bottom_panel_item_visible(control)


func _current_script_path() -> String:
	var script := EditorInterface.get_script_editor().get_current_script()
	return "" if script == null else script.resource_path


func _current_code_edit() -> CodeEdit:
	var editor := EditorInterface.get_script_editor().get_current_editor()
	if editor == null:
		return null
	return editor.get_base_editor() as CodeEdit


## The editor showing the script at `path`, or null when it isn't open.
func _code_edit_for(path: String) -> CodeEdit:
	var script_editor := EditorInterface.get_script_editor()
	var scripts := script_editor.get_open_scripts()
	var editors := script_editor.get_open_script_editors()
	if scripts.size() != editors.size():
		return null
	for index in scripts.size():
		if scripts[index].resource_path == path:
			return editors[index].get_base_editor() as CodeEdit
	return null


func _toast(message: String, warning := false) -> void:
	var toaster := EditorInterface.get_editor_toaster()
	var severity := EditorToaster.SEVERITY_WARNING if warning else EditorToaster.SEVERITY_INFO
	toaster.push_toast("GDSuite: " + message, severity)


func _icon(icon_name: String) -> Texture2D:
	return EditorInterface.get_editor_theme().get_icon(icon_name, "EditorIcons")
