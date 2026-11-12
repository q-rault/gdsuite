@tool
extends EditorPlugin
## GDSuite in the Godot editor: a Check button, a Problems dock, problems
## marked in the script editor, and formatting. Everything runs the gdsuite
## program, with the project's gdsuite.toml, so the editor agrees with the
## terminal and CI.

const Runner = preload("runner.gd")
const Settings = preload("settings.gd")
const Markers = preload("markers.gd")
const ProblemsDock = preload("problems_dock.gd")

const DOCS := "https://q-rault.github.io/gdsuite"
const NOT_FOUND := "gdsuite not found: install it, or set its path in Editor Settings > GDSuite"
const INSTALL_DOCS := DOCS + "/install"
const RULE_DOCS := DOCS + "/reference/lint-rules#"
## Covers its own line and the next, as gdtidy reads it.
const IGNORE_COMMENT := "  # gdtidy: ignore=%s"

const SHORTCUT_FORMAT := "gdsuite/format_script"
const COMMAND_FORMAT := "gdsuite/format_script"
const COMMAND_CHECK := "gdsuite/check"
const COMMAND_PROBLEMS := "gdsuite/show_problems"
const COMMAND_REFRESH := "gdsuite/refresh_problems"

const MENU_FORMAT := 0
const MENU_CHECK := 1
const MENU_PROBLEMS := 2
const MENU_REFRESH := 3
const MENU_CONFIG := 4

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
var _check_button: Button
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
	if ClassDB.class_exists("EditorDock"):
		_dock_host = ClassDB.instantiate("EditorDock")
		_dock_host.set("title", "Problems")
		_dock_host.set("layout_key", "gdsuite_problems")
		_dock_host.set("default_slot", SLOT_BOTTOM)
		_dock_host.set("available_layouts", LAYOUT_HORIZONTAL | LAYOUT_FLOATING)
		_dock_host.add_child(_dock)
		call("add_dock", _dock_host)
	else:
		add_control_to_bottom_panel(_dock, "Problems")

	_check_button = Button.new()
	_check_button.text = "Check"
	_check_button.flat = true
	_check_button.tooltip_text = "Run the project's check: the steps of [check] in gdsuite.toml"
	_check_button.pressed.connect(_run_check)
	add_control_to_container(CONTAINER_TOOLBAR, _check_button)

	_menu = PopupMenu.new()
	_menu.add_item("Format script", MENU_FORMAT)
	_menu.add_item("Check the project", MENU_CHECK)
	_menu.add_item("Show problems", MENU_PROBLEMS)
	_menu.add_item("Lint and type check the project", MENU_REFRESH)
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

	_type_check_timer = Timer.new()
	_type_check_timer.one_shot = true
	_type_check_timer.wait_time = 1.5
	_type_check_timer.timeout.connect(_type_check)
	add_child(_type_check_timer)

	resource_saved.connect(_on_resource_saved)
	var script_editor := EditorInterface.get_script_editor()
	script_editor.editor_script_changed.connect(_on_script_changed)
	EditorInterface.get_resource_filesystem().filesystem_changed.connect(_read_project_settings)

	if _runner.find_program() == "":
		_dock.set_status(NOT_FOUND)
		_set_check_state("", "gdsuite not found. Install GDSuite: " + INSTALL_DOCS)
		return
	_read_project_settings()
	_set_check_state("", "Run the project's check")
	_refresh_project()


func _exit_tree() -> void:
	resource_saved.disconnect(_on_resource_saved)
	var script_editor := EditorInterface.get_script_editor()
	if script_editor.editor_script_changed.is_connected(_on_script_changed):
		script_editor.editor_script_changed.disconnect(_on_script_changed)
	var filesystem := EditorInterface.get_resource_filesystem()
	if filesystem.filesystem_changed.is_connected(_read_project_settings):
		filesystem.filesystem_changed.disconnect(_read_project_settings)
	for id: int in _connected_code_edits:
		var code_edit := instance_from_id(id) as CodeEdit
		if is_instance_valid(code_edit):
			_markers.clear(code_edit)
			code_edit.gutter_clicked.disconnect(_connected_code_edits[id])
	_connected_code_edits.clear()

	var palette := EditorInterface.get_command_palette()
	for command in [COMMAND_FORMAT, COMMAND_CHECK, COMMAND_PROBLEMS, COMMAND_REFRESH]:
		palette.remove_command(command)
	remove_tool_menu_item("GDSuite")
	remove_control_from_container(CONTAINER_TOOLBAR, _check_button)
	_check_button.queue_free()
	if _dock_host != null:
		call("remove_dock", _dock_host)
		_dock_host.queue_free()
	else:
		remove_control_from_bottom_panel(_dock)
		_dock.queue_free()
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
	_check_button.disabled = true
	_set_check_state("Reload", "Checking...")
	_runner.run(PackedStringArray(["check", "--report", "json"]), _on_check_done)


func _on_check_done(result: Dictionary) -> void:
	_check_button.disabled = false
	var report: Variant = result.json
	if not report is Dictionary:
		var output := str(result.output).strip_edges()
		_set_check_state("StatusError", "The check could not run:\n" + output)
		return
	var lines := PackedStringArray()
	for step: Dictionary in report.get("steps", []):
		var mark := "✔" if step.get("status") == "ok" else "✖"
		lines.append("%s %s: %s" % [mark, step.get("name", ""), step.get("summary", "")])
	var passed: bool = report.get("status") == "ok"
	_set_check_state("StatusSuccess" if passed else "StatusError", "\n".join(lines))
	_refresh_project()
	if not passed:
		_show_dock()


func _set_check_state(icon_name: String, tooltip: String) -> void:
	_check_button.icon = null if icon_name == "" else _icon(icon_name)
	_check_button.tooltip_text = tooltip


# --- Problems ---------------------------------------------------------------


func _refresh_project() -> void:
	_lint(PackedStringArray())
	_type_check()


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


## Fixes the script on disk, then shows the result in the open editor. A
## script with unsaved changes is left alone: they would be lost.
func _fix(problem: Dictionary) -> void:
	var code_edit := _code_edit_for(problem.file)
	if code_edit != null and code_edit.get_version() != code_edit.get_saved_version():
		_toast("Save the script first: fixing works on the saved file", true)
		return
	_runner.run(
		PackedStringArray(["lint", "--", "--fix", problem.file.trim_prefix("res://")]),
		func(_result: Dictionary) -> void:
			var text := FileAccess.get_file_as_string(problem.file)
			var script := load(problem.file) as GDScript
			if script != null:
				script.source_code = text
			if code_edit != null and is_instance_valid(code_edit) and text != "":
				_replace_text(code_edit, text, true)
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
		return
	_markers.show(code_edit, _dock.problems_for(path))


func _on_script_changed(_script: Script) -> void:
	_dock.set_current_script(_current_script_path())
	var code_edit := _current_code_edit()
	if code_edit != null and not _connected_code_edits.has(code_edit.get_instance_id()):
		var on_click := _on_gutter_clicked.bind(code_edit)
		code_edit.gutter_clicked.connect(on_click)
		_connected_code_edits[code_edit.get_instance_id()] = on_click
	_show_markers()


func _on_gutter_clicked(line: int, gutter: int, code_edit: CodeEdit) -> void:
	var problems := _markers.problems_at(code_edit, line, gutter)
	if problems.is_empty():
		return
	var messages := PackedStringArray()
	for problem: Dictionary in problems:
		messages.append("%s (%s)" % [problem.message, problem.rule])
	var has_error := problems.any(func(p: Dictionary) -> bool: return p.severity == "error")
	_toast("\n".join(messages), has_error)


# --- Saving and formatting --------------------------------------------------


func _on_resource_saved(resource: Resource) -> void:
	if _saving or not resource is GDScript or _runner.program == "":
		return
	var path := resource.resource_path
	# Built-in scripts are saved with their scene.
	if path == "" or path.contains("::"):
		return
	if Settings.value(Settings.FORMAT_ON_SAVE):
		_format_saved(resource as GDScript)
	if Settings.value(Settings.LINT_ON_SAVE):
		_lint(PackedStringArray([path]))
	if Settings.value(Settings.TYPE_CHECK_ON_SAVE):
		_type_check_timer.start()


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
	)


func _on_setting_toggled(key: String, _enabled: bool) -> void:
	if key == Settings.SHOW_IN_GUTTER:
		_show_markers()


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


func _show_dock() -> void:
	if _dock_host != null:
		_dock_host.call("make_visible")
	else:
		make_bottom_panel_item_visible(_dock)


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
