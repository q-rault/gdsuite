@tool
extends EditorPlugin
## GDSuite in the Godot editor: a Check and a Tests icon in the top bar, a
## Problems and a Tests dock, and problems and tests marked in the script
## editor. Everything runs the gdsuite program, with the project's
## gdsuite.toml, so the editor agrees with the terminal and CI.

const Runner = preload("runner.gd")
const Settings = preload("settings.gd")
const Markers = preload("markers.gd")
const ProblemsDock = preload("problems_dock.gd")
const Problem = preload("problem.gd")
const Ui = preload("ui.gd")
const TestsDock = preload("tests_dock.gd")
const TestsGutter = preload("tests_gutter.gd")
const TestsMenu = preload("tests_menu.gd")
const Checker = preload("checker.gd")
const ScriptEdits = preload("script_edits.gd")
const Status = preload("status.gd")

const NOT_FOUND := "gdsuite not found: install it, or set its path in Editor Settings > GDSuite"
## The page of the full code health report.
const HEALTH_REPORT := "reports/code-health/browser/emerge.html"
## Covers its own line and the next, as gdtidy reads it.
const IGNORE_COMMENT := "  # gdtidy: ignore=%s"

const SHORTCUT_FORMAT := "gdsuite/format_script"
const COMMAND_FORMAT := "gdsuite/format_script"
const COMMAND_CHECK := "gdsuite/check"
const COMMAND_PROBLEMS := "gdsuite/show_problems"
const COMMAND_REPORT := "gdsuite/health_report"
const COMMAND_RUN_TESTS := "gdsuite/run_all_tests"
const COMMAND_RERUN_FAILED := "gdsuite/rerun_failed_tests"
const COMMAND_RUN_SCRIPT_TESTS := "gdsuite/run_script_tests"
const COMMAND_SHOW_TESTS := "gdsuite/show_tests"
const COMMAND_NEXT_PROBLEM := "gdsuite/next_problem"
const COMMAND_PREVIOUS_PROBLEM := "gdsuite/previous_problem"
const SHORTCUT_NEXT_PROBLEM := "gdsuite/next_problem"
const SHORTCUT_PREVIOUS_PROBLEM := "gdsuite/previous_problem"
const COMMANDS := [
	COMMAND_FORMAT,
	COMMAND_CHECK,
	COMMAND_PROBLEMS,
	COMMAND_REPORT,
	COMMAND_RUN_TESTS,
	COMMAND_RERUN_FAILED,
	COMMAND_RUN_SCRIPT_TESTS,
	COMMAND_SHOW_TESTS,
	COMMAND_NEXT_PROBLEM,
	COMMAND_PREVIOUS_PROBLEM,
]

const MENU_FORMAT := 0
const MENU_CHECK := 1
const MENU_PROBLEMS := 2
const MENU_REPORT := 3
const MENU_CONFIG := 4
const MENU_RUN_TESTS := 5
const MENU_TESTS := 6

## EditorDock.DockSlot.SLOT_BOTTOM and DockLayout flags, written as
## numbers so the addon also loads on Godot 4.5, which has no EditorDock.
const SLOT_BOTTOM := 8
const LAYOUT_HORIZONTAL := 2
const LAYOUT_FLOATING := 4

var _runner := Runner.new()
var _checker := Checker.new()
var _markers := Markers.new()
var _dock: ProblemsDock
## The EditorDock holding `_dock` on Godot 4.6 and newer; null on 4.5.
var _dock_host: Control
var _tests: TestsDock
var _tests_host: Control
## The grey dots of the docks' tabs, by colour: see `_dot`.
var _dots := {}
var _tests_gutter := TestsGutter.new()
var _tests_menu: TestsMenu
## Waits for the file system to settle before finding the tests again.
var _discover_timer: Timer
var _check_button: Button
var _tests_button: Button
## The icons the top bar and the docks' buttons are drawn from, made once:
## Status keeps what it draws from them by their resource.
var _check_icon: Texture2D
var _tests_icon: Texture2D
var _stop_icon: Texture2D
## Turns the spinners of the top bar and the docks' tabs while a check or the
## tests run.
var _spinner: Timer
var _spinner_frame := 0
var _menu: PopupMenu
var _format_shortcut: Shortcut
var _next_shortcut: Shortcut
var _previous_shortcut: Shortcut
var _connected_code_edits := {}


func _get_plugin_name() -> String:
	return "GDSuite"


func _enter_tree() -> void:
	Settings.register()

	_dock = ProblemsDock.new()
	_dock.action_requested.connect(_on_problem_action)
	_dock.check_requested.connect(_check)
	_dock.report_requested.connect(_open_health_report)
	_dock.problems_changed.connect(_show_markers)
	_dock.setting_toggled.connect(_on_setting_toggled)
	_dock.open_config_requested.connect(_open_config)
	_dock_host = _add_dock(_dock, "Problems", "gdsuite_problems")

	_checker.runner = _runner
	_checker.dock = _dock
	add_child(_checker)

	_tests = TestsDock.new()
	_tests.runner = _runner
	_tests.watch.compile_state = _checker.compile_state
	_checker.compiled.connect(_tests.watch.run_waiting)
	_tests.results_changed.connect(_show_test_marks)
	_tests.open_requested.connect(_open_test)
	_tests.open_line_requested.connect(ScriptEdits.open)
	_tests.test_all_requested.connect(_test_all)
	_tests.setting_toggled.connect(_on_setting_toggled)
	_tests.open_config_requested.connect(_open_config)
	_tests.results_changed.connect(_show_tests_state)
	_tests.running_changed.connect(_show_tests_state)
	_tests_host = _add_dock(_tests, "Tests", "gdsuite_tests")
	_tests_menu = TestsMenu.new()
	_tests_menu.run_paths = _run_tests_and_show
	add_context_menu_plugin(EditorContextMenuPlugin.CONTEXT_SLOT_FILESYSTEM, _tests_menu)

	_check_icon = Ui.svg_icon(Ui.CHECK_ICON)
	_tests_icon = Ui.svg_icon(Ui.TESTS_ICON)
	_stop_icon = Ui.icon("Stop")
	_check_button = _status_button(_check_icon, _check)
	_tests_button = _status_button(_tests_icon, _test_all)
	_spinner = Timer.new()
	_spinner.wait_time = 0.1
	_spinner.timeout.connect(_on_spinner)
	add_child(_spinner)
	add_control_to_container(CONTAINER_TOOLBAR, _check_button)
	add_control_to_container(CONTAINER_TOOLBAR, _tests_button)
	_check_button.visible = Settings.value(Settings.TOP_BAR_CHECK)
	_tests_button.visible = Settings.value(Settings.TOP_BAR_TESTS)
	_dock.problems_changed.connect(_show_check_state)
	_dock.state_changed.connect(_show_check_state)
	_checker.checking_changed.connect(_show_check_state)

	_menu = PopupMenu.new()
	_menu.add_item("Format script", MENU_FORMAT)
	_menu.add_item("Check the project", MENU_CHECK)
	_menu.add_item("Show problems", MENU_PROBLEMS)
	_menu.add_item("Open the full health report", MENU_REPORT)
	_menu.add_separator()
	_menu.add_item("Test all", MENU_RUN_TESTS)
	_menu.add_item("Show tests", MENU_TESTS)
	_menu.add_separator()
	_menu.add_item("Open gdsuite.toml", MENU_CONFIG)
	_menu.id_pressed.connect(_on_menu)
	add_tool_submenu_item("GDSuite", _menu)

	_format_shortcut = Ui.register_shortcut(SHORTCUT_FORMAT, KEY_F, true)
	# Not F8, as in VS Code: Godot stops the running project with it.
	_next_shortcut = Ui.register_shortcut(SHORTCUT_NEXT_PROBLEM, KEY_F8, false)
	_previous_shortcut = Ui.register_shortcut(SHORTCUT_PREVIOUS_PROBLEM, KEY_F8, true)
	var palette := EditorInterface.get_command_palette()
	var format_keys := _format_shortcut.get_as_text()
	var format := _checker.format_current_script
	palette.add_command("Format Script", COMMAND_FORMAT, format, format_keys)
	palette.add_command("Check the Project", COMMAND_CHECK, _check)
	palette.add_command("Show Problems", COMMAND_PROBLEMS, _show_dock)
	palette.add_command("Open the Full Health Report", COMMAND_REPORT, _open_health_report)
	var run_all := _run_tests_and_show.bind(PackedStringArray())
	palette.add_command("Test All", COMMAND_RUN_TESTS, run_all)
	palette.add_command("Rerun Failed Tests", COMMAND_RERUN_FAILED, _tests.rerun_failed)
	palette.add_command("Run Tests in Script", COMMAND_RUN_SCRIPT_TESTS, _run_script_tests)
	palette.add_command("Show Tests", COMMAND_SHOW_TESTS, _show_tests)
	var next := _step_problem.bind(1)
	var previous := _step_problem.bind(-1)
	palette.add_command("Next Problem", COMMAND_NEXT_PROBLEM, next, _next_shortcut.get_as_text())
	var previous_keys := _previous_shortcut.get_as_text()
	palette.add_command("Previous Problem", COMMAND_PREVIOUS_PROBLEM, previous, previous_keys)

	_discover_timer = Timer.new()
	_discover_timer.one_shot = true
	_discover_timer.wait_time = 1.0
	_discover_timer.timeout.connect(_tests.discover)
	add_child(_discover_timer)

	var editor_settings := EditorInterface.get_editor_settings()
	editor_settings.settings_changed.connect(_on_editor_settings_changed)
	resource_saved.connect(_on_resource_saved)
	scene_saved.connect(_on_scene_saved)
	var script_editor := EditorInterface.get_script_editor()
	script_editor.editor_script_changed.connect(_on_script_changed)
	EditorInterface.get_resource_filesystem().filesystem_changed.connect(_on_filesystem_changed)

	if _runner.find_program() == "":
		_dock.set_status(NOT_FOUND)
		var install := "gdsuite not found. Install GDSuite: " + Problem.INSTALL_DOCS
		var none := Status.RESULT_NONE
		_paint([_check_button, _dock.action_button()], _check_icon, none, Status.FRESH, install)
		_paint([_tests_button, _tests.action_button()], _tests_icon, none, Status.FRESH, install)
		return
	_read_project_settings()
	_show_check_state()
	_show_tests_state()
	# Opening the project only looks: Check is what fixes.
	_checker.check(false)


func _exit_tree() -> void:
	var editor_settings := EditorInterface.get_editor_settings()
	editor_settings.settings_changed.disconnect(_on_editor_settings_changed)
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
			code_edit.gutter_clicked.disconnect(_connected_code_edits[id][0])
			code_edit.text_changed.disconnect(_connected_code_edits[id][1])
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
		_checker.format_current_script()
		get_viewport().set_input_as_handled()
	elif _next_shortcut.matches_event(event):
		_step_problem(1)
		get_viewport().set_input_as_handled()
	elif _previous_shortcut.matches_event(event):
		_step_problem(-1)
		get_viewport().set_input_as_handled()


## Opens the problem after (1) or before (-1) the caret, in the order of the
## Problems dock.
func _step_problem(direction: int) -> void:
	var code_edit := ScriptEdits.current_code_edit()
	var line := 0
	if code_edit != null and EditorInterface.get_script_editor().is_visible_in_tree():
		line = code_edit.get_caret_line() + 1
	var problem := _dock.step(ScriptEdits.current_path(), line, direction)
	if problem.is_empty():
		ScriptEdits.toast("No problems")
		return
	_open(problem)


# --- Check ------------------------------------------------------------------


## Checks the project, and fixes what can be fixed unless Autofix is off: the
## Check icon of the top bar and the Check button of the Problems dock.
func _check() -> void:
	if _runner.program == "":
		ScriptEdits.toast("gdsuite not found. Install GDSuite: " + Problem.INSTALL_DOCS, true)
		return
	_show_dock()
	_checker.check(bool(Settings.value(Settings.AUTOFIX)))


## The Check icon of the top bar and the Check button of the Problems dock:
## grey until every tool has reported, then a red cross for errors, a yellow
## "!" for warnings, a green check for none; faded when files changed since,
## with a spinner while a tool checks.
func _show_check_state() -> void:
	var lines := PackedStringArray()
	var result := _dock.result()
	var freshness := _dock.freshness()
	var covered := PackedStringArray()
	for source: String in Problem.SOURCES:
		if Problem.is_checked(source):
			covered.append(Problem.TOOL_NAMES[source])
	if result == Status.RESULT_NONE:
		lines.append("Checking…" if freshness == Status.RUNNING else "Not checked yet")
	else:
		var count := _dock.store.counts()
		var found := "No problems"
		if count.errors + count.warnings > 0:
			found = "%s, %s" % [_plural(count.errors, "error"), _plural(count.warnings, "warning")]
		lines.append(found)
		if freshness == Status.RUNNING:
			lines.append("Checking again…")
		elif freshness == Status.STALE:
			var changed := _dock.store.changed_files().size()
			lines.append("Out of date: %s changed since" % _plural(changed, "file"))
	var fixes := ", and fix what can be fixed" if Settings.value(Settings.AUTOFIX) else ""
	lines.append("Click to check the project%s: %s" % [fixes, ", ".join(covered)])
	var tooltip := "\n".join(lines)
	_paint([_check_button, _dock.action_button()], _check_icon, result, freshness, tooltip)
	_check_button.disabled = _checker.checking
	_dock.set_check(_checker.checking, tooltip)
	_update_titles()


## The Tests icon of the top bar and the Test all button of the Tests dock:
## grey until tests ran, then a red cross when one fails, a green check when
## none does; faded when some have not run since their scripts changed. While
## they run, a Stop icon with a spinner: clicking stops them.
func _show_tests_state() -> void:
	# The Tests dock reports before the top bar has its icon.
	if _tests_button == null:
		return
	var freshness := _tests.freshness()
	var running := freshness == Status.RUNNING
	var action := "Click to stop the tests" if running else "Click to run all tests"
	var tooltip := _tests.summary() + "\n" + action
	var base := _stop_icon if running else _tests_icon
	_paint([_tests_button, _tests.action_button()], base, _tests.result(), freshness, tooltip)
	_update_titles()


## The docks' tabs show what the top bar shows, as the badge alone: a red
## cross, a yellow "!", a green check, faded when out of date, a spinner while
## running, a grey dot when nothing is known yet. Godot 4.5's bottom panels
## keep their titles.
func _update_titles() -> void:
	if _dock_host != null:
		_set_tab_icon(_dock_host, _dock.result(), _dock.freshness())
	if _tests_host != null:
		_set_tab_icon(_tests_host, _tests.result(), _tests.freshness())
	var running := _dock.freshness() == Status.RUNNING or _tests.freshness() == Status.RUNNING
	if running and _spinner.is_stopped() and _spinner.is_inside_tree():
		_spinner.start()
	elif not running:
		_spinner.stop()


func _set_tab_icon(host: Object, result: String, freshness: String) -> void:
	var icon := Status.tab_icon(result, freshness, _spinner_frame)
	if icon == null:
		var theme := EditorInterface.get_editor_theme()
		icon = _dot(Color(theme.get_color("font_color", "Editor"), 0.5))
	host.set("dock_icon", icon)
	host.set("force_show_icon", true)
	host.set("title_color", Color(0, 0, 0, 0))


## A dot of `color` before a dock's name, as the Output dock has: grey, for a
## dock with nothing known yet.
func _dot(color: Color) -> Texture2D:
	var key := color.to_html()
	if not _dots.has(key):
		var theme := EditorInterface.get_editor_theme()
		var image := theme.get_icon("Warning", "EditorIcons").get_image()
		image.convert(Image.FORMAT_RGBA8)
		for y in image.get_height():
			for x in image.get_width():
				var alpha := image.get_pixel(x, y).a
				image.set_pixel(x, y, Color(color, color.a * alpha))
		_dots[key] = ImageTexture.create_from_image(image)
	return _dots[key]


func _on_spinner() -> void:
	_spinner_frame = (_spinner_frame + 1) % 8
	if _dock.freshness() == Status.RUNNING:
		_show_check_state()
	if _tests.freshness() == Status.RUNNING:
		_show_tests_state()


static func _plural(count: int, word: String) -> String:
	return "%d %s%s" % [count, word, "" if count == 1 else "s"]


## Runs every test, or stops the tests: the Tests icon of the top bar and the
## Test all button of the Tests dock.
func _test_all() -> void:
	if _tests.is_running():
		_tests.stop()
	else:
		_run_tests_and_show(PackedStringArray())


func _status_button(icon: Texture2D, pressed: Callable) -> Button:
	var button := Button.new()
	button.flat = true
	button.icon = icon
	button.pressed.connect(pressed)
	return button


## Paints a top bar icon and the dock's button that does the same: `base` in
## the colour of `result` with its badge, see Status.icon.
func _paint(
	buttons: Array, base: Texture2D, result: String, freshness: String, tooltip: String
) -> void:
	var icon := Status.icon(base, result, freshness, _spinner_frame)
	for button: Button in buttons:
		if button == null:
			continue
		button.icon = icon
		# The icon has its colours: the theme's tint would change them.
		for color_name: String in [
			"icon_normal_color",
			"icon_hover_color",
			"icon_pressed_color",
			"icon_hover_pressed_color",
			"icon_focus_color",
		]:
			button.add_theme_color_override(color_name, Color.WHITE)
		button.tooltip_text = tooltip


# --- Problems ---------------------------------------------------------------


func _on_problem_action(action: String, problem: Dictionary) -> void:
	match action:
		"open":
			_open(problem)
		"fix":
			_checker.fix([problem.file], [])
		"format_all":
			_checker.fix([], problem.files)
		"ignore_here":
			_ignore_here(problem)
		"disable_rule":
			_runner.run(
				PackedStringArray(["config", "add", "lint.disable", problem.rule]),
				func(result: Dictionary) -> void:
					if result.code != 0:
						var why := str(result.output)
						ScriptEdits.toast("Could not change gdsuite.toml: " + why, true)
						return
					ScriptEdits.toast('"%s" is turned off in gdsuite.toml' % problem.rule)
					_checker.lint(PackedStringArray())
			)
		"docs":
			var is_lint: bool = problem.source == Problem.SOURCE_LINT
			OS.shell_open(Problem.RULE_DOCS + problem.rule if is_lint else Problem.HEALTH_DOCS)


func _open(problem: Dictionary) -> void:
	ScriptEdits.open(problem.file, int(problem.line), int(problem.get("column", 1)))


## Writes the full code health report, with Git history, and opens it.
func _open_health_report() -> void:
	if _runner.program == "":
		return
	ScriptEdits.toast("Writing the health report…")
	_runner.run(
		PackedStringArray(["health"]),
		func(result: Dictionary) -> void:
			var path := Runner.project_root().path_join(HEALTH_REPORT)
			if result.code == 0 and FileAccess.file_exists(path):
				OS.shell_open(path)
			else:
				ScriptEdits.toast("No health report: " + str(result.output).strip_edges(), true)
	)


func _ignore_here(problem: Dictionary) -> void:
	_open(problem)
	var code_edit := ScriptEdits.code_edit_for(problem.file)
	if code_edit == null:
		return
	var line := int(problem.line) - 1
	code_edit.begin_complex_operation()
	code_edit.set_line(line, code_edit.get_line(line) + IGNORE_COMMENT % problem.rule)
	code_edit.end_complex_operation()


func _show_markers() -> void:
	var path := ScriptEdits.current_path()
	var code_edit := ScriptEdits.current_code_edit()
	if code_edit == null:
		return
	if not Settings.value(Settings.SHOW_IN_GUTTER):
		_markers.clear(code_edit)
	else:
		# A script that is not formatted has no line to mark.
		var problems := _dock.store.problems_for(path).filter(
			func(p: Dictionary) -> bool: return p.source != Problem.SOURCE_FORMAT
		)
		_markers.show(code_edit, problems)


func _on_script_changed(_script: Script) -> void:
	_dock.set_current_script(ScriptEdits.current_path())
	_tests.watch.remember(ScriptEdits.current_path())
	var code_edit := ScriptEdits.current_code_edit()
	if code_edit != null and not _connected_code_edits.has(code_edit.get_instance_id()):
		var on_click := _on_gutter_clicked.bind(code_edit)
		code_edit.gutter_clicked.connect(on_click)
		# Its problems may be gone once edited: faded until checked again.
		var on_edit := _on_text_changed.bind(code_edit, ScriptEdits.current_path())
		code_edit.text_changed.connect(on_edit)
		_connected_code_edits[code_edit.get_instance_id()] = [on_click, on_edit]
	_show_markers()
	_show_test_marks()


func _on_text_changed(code_edit: CodeEdit, path: String) -> void:
	if not _checker.saving and code_edit.get_version() != code_edit.get_saved_version():
		_dock.mark_stale(path)


func _on_gutter_clicked(line: int, gutter: int, code_edit: CodeEdit) -> void:
	var test_name := _tests_gutter.test_at(code_edit, line, gutter)
	if test_name != "":
		_run_tests_and_show(PackedStringArray(["%s::%s" % [ScriptEdits.current_path(), test_name]]))
		return
	var problems := _markers.problems_at(code_edit, line, gutter)
	if problems.is_empty():
		return
	var messages := PackedStringArray()
	for problem: Dictionary in problems:
		messages.append("%s (%s)" % [problem.message, problem.rule])
	var has_error := problems.any(func(p: Dictionary) -> bool: return p.severity == "error")
	ScriptEdits.toast("\n".join(messages), has_error)


# --- Tests ------------------------------------------------------------------


func _run_tests_and_show(targets: PackedStringArray) -> void:
	if Runner.gdtest_program() == "":
		ScriptEdits.toast("Run `gdsuite test` once in a terminal to set up the tests", true)
		return
	_tests.run(targets)
	_show_tests()


func _run_script_tests() -> void:
	var path := ScriptEdits.current_path()
	if path == "" or _tests.tests_of(path).is_empty():
		ScriptEdits.toast("The current script has no tests")
		return
	_run_tests_and_show(PackedStringArray([path]))


## The test marks in the gutter of the script being edited.
func _show_test_marks() -> void:
	var code_edit := ScriptEdits.current_code_edit()
	if code_edit == null:
		return
	var tests := _tests.tests_of(ScriptEdits.current_path())
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
	var code_edit := ScriptEdits.code_edit_for(file)
	var line := -1 if code_edit == null else TestsGutter.line_of(code_edit, test_name)
	if line >= 0:
		EditorInterface.edit_script(script, line + 1)


func _on_filesystem_changed() -> void:
	_read_project_settings()
	_discover_timer.start()


# --- Saving ------------------------------------------------------------------


func _on_resource_saved(resource: Resource) -> void:
	if _checker.saving or _runner.program == "":
		return
	var path := resource.resource_path
	# Built-in scripts and resources are saved with their scene.
	if path == "" or path.contains("::"):
		return
	if not resource is GDScript:
		_tests.watch.file_saved(path)
		return
	_checker.script_saved(resource as GDScript)
	_tests.watch.file_saved(path)


func _on_scene_saved(path: String) -> void:
	if not _checker.saving:
		_tests.watch.file_saved(path)


# --- Settings ---------------------------------------------------------------


func _read_project_settings() -> void:
	if _runner.program == "":
		return
	_runner.run(
		PackedStringArray(["config", "get", "editor"]),
		func(result: Dictionary) -> void:
			var values: Dictionary = result.json if result.json is Dictionary else {}
			if values != Settings.project_values:
				Settings.project_values = values
				_show_settings()
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


func _on_setting_toggled(key: String) -> void:
	_show_settings()
	# What was turned on has nothing to show yet.
	var source: Variant = Problem.SETTINGS.find_key(key)
	if source != null and Problem.is_checked(source):
		_checker.check_source(source)


func _show_settings() -> void:
	_dock.show_settings()
	_tests.show_settings()
	_show_markers()
	_check_button.visible = Settings.value(Settings.TOP_BAR_CHECK)
	_tests_button.visible = Settings.value(Settings.TOP_BAR_TESTS)
	_show_check_state()


## Editor Settings changed, maybe from its own window.
func _on_editor_settings_changed() -> void:
	var settings := EditorInterface.get_editor_settings()
	for key in settings.get_changed_settings():
		if key.begins_with("gdsuite/"):
			_show_settings()
			return


func _open_config() -> void:
	var path := Runner.project_root().path_join("gdsuite.toml")
	if not FileAccess.file_exists(path):
		ScriptEdits.toast(
			"This project has no gdsuite.toml: GDSuite uses its defaults. "
			+ "Run `gdsuite init` to write one."
		)
		return
	OS.shell_open(path)


# --- Helpers ----------------------------------------------------------------


func _on_menu(id: int) -> void:
	match id:
		MENU_FORMAT:
			_checker.format_current_script()
		MENU_CHECK:
			_check()
		MENU_PROBLEMS:
			_show_dock()
		MENU_REPORT:
			_open_health_report()
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
