@tool
extends VBoxContainer
## The Problems dock: what lint and the type check found, in one list grouped
## by script. A problem stays listed until a new run says it's gone, whichever
## scripts are open.

signal problems_changed
## Asks the plugin to run something on a problem: "fix", "disable_rule",
## "ignore_here", "docs".
signal action_requested(action: String, problem: Dictionary)
## Asks the plugin to look again: lint and type check the whole project.
signal refresh_requested
signal setting_toggled(key: String, enabled: bool)
signal open_config_requested

const Settings = preload("settings.gd")

const SOURCE_LINT := "lint"
const SOURCE_TYPES := "types"
const SCOPE_PROJECT := 0
const SCOPE_SCRIPT := 1

const MENU_OPEN := 0
const MENU_FIX := 1
const MENU_IGNORE := 2
const MENU_DISABLE := 3
const MENU_DOCS := 4
const MENU_COPY := 5
const MENU_OPEN_CONFIG := 100

const SETTING_IDS := [
	Settings.FORMAT_ON_SAVE,
	Settings.LINT_ON_SAVE,
	Settings.TYPE_CHECK_ON_SAVE,
	Settings.SHOW_IN_GUTTER,
]

## source -> { "res://path.gd": [problem, ...] }. A problem is a Dictionary:
## {file, line, column, severity, message, rule, fixable, source}.
var _problems := { SOURCE_LINT: {}, SOURCE_TYPES: {} }
var _current_script := ""

var _tree: Tree
var _status: Label
var _errors_button: Button
var _warnings_button: Button
var _lint_button: Button
var _types_button: Button
var _scope: OptionButton
var _settings_menu: MenuButton
var _context_menu: PopupMenu
var _context_problem := {}


func _ready() -> void:
	name = "Problems"
	size_flags_vertical = SIZE_EXPAND_FILL
	custom_minimum_size = Vector2(0, 120)

	var bar := HBoxContainer.new()
	add_child(bar)

	_errors_button = _toggle(bar, "Errors", "StatusError", "Show errors")
	_warnings_button = _toggle(bar, "Warnings", "StatusWarning", "Show warnings")
	bar.add_child(VSeparator.new())
	_lint_button = _toggle(bar, "Lint", "", "Show lint findings")
	_types_button = _toggle(bar, "Types", "", "Show type check findings")
	bar.add_child(VSeparator.new())

	_scope = OptionButton.new()
	_scope.add_item("Whole project", SCOPE_PROJECT)
	_scope.add_item("Current script", SCOPE_SCRIPT)
	_scope.tooltip_text = "Which scripts to list"
	_scope.item_selected.connect(func(_index: int) -> void: _rebuild())
	bar.add_child(_scope)

	_status = Label.new()
	_status.size_flags_horizontal = SIZE_EXPAND_FILL
	_status.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_status.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	bar.add_child(_status)

	var refresh := Button.new()
	refresh.flat = true
	refresh.icon = _icon("Reload")
	refresh.tooltip_text = "Lint and type check the whole project"
	refresh.pressed.connect(func() -> void: refresh_requested.emit())
	bar.add_child(refresh)

	_settings_menu = MenuButton.new()
	_settings_menu.flat = true
	_settings_menu.icon = _icon("Tools")
	_settings_menu.tooltip_text = "GDSuite settings"
	_settings_menu.about_to_popup.connect(_fill_settings_menu)
	_settings_menu.get_popup().id_pressed.connect(_on_settings_menu)
	bar.add_child(_settings_menu)

	_tree = Tree.new()
	_tree.size_flags_vertical = SIZE_EXPAND_FILL
	_tree.hide_root = true
	_tree.columns = 3
	_tree.column_titles_visible = true
	_tree.set_column_title(0, "Where")
	_tree.set_column_title(1, "Problem")
	_tree.set_column_title(2, "Rule")
	_tree.set_column_expand(0, false)
	_tree.set_column_custom_minimum_width(0, 220)
	_tree.set_column_expand(2, false)
	_tree.set_column_custom_minimum_width(2, 200)
	_tree.allow_rmb_select = true
	_tree.item_activated.connect(_on_item_activated)
	_tree.item_mouse_selected.connect(_on_item_mouse_selected)
	add_child(_tree)

	_context_menu = PopupMenu.new()
	_context_menu.id_pressed.connect(_on_context_menu)
	add_child(_context_menu)

	_rebuild()


## Replaces the lint findings of `files` ("res://" paths), or of the whole
## project when `files` is empty.
func set_lint(files: PackedStringArray, problems: Array) -> void:
	_replace(SOURCE_LINT, files, problems)


## Replaces every type check finding: the type check covers the project.
func set_types(problems: Array) -> void:
	_replace(SOURCE_TYPES, PackedStringArray(), problems)


func set_status(text: String) -> void:
	if is_instance_valid(_status):
		_status.text = text


func set_current_script(path: String) -> void:
	_current_script = path
	if is_instance_valid(_scope) and _scope.selected == SCOPE_SCRIPT:
		_rebuild()


## The problems of one script, both sources, sorted by line.
func problems_for(path: String) -> Array:
	var found := []
	for source: String in _problems:
		found.append_array(_problems[source].get(path, []))
	found.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return a.line < b.line)
	return found


## {errors, warnings} over the whole project.
func counts() -> Dictionary:
	var errors := 0
	var warnings := 0
	for source: String in _problems:
		for path: String in _problems[source]:
			for problem: Dictionary in _problems[source][path]:
				if problem.severity == "error":
					errors += 1
				else:
					warnings += 1
	return { "errors": errors, "warnings": warnings }


func _replace(source: String, files: PackedStringArray, problems: Array) -> void:
	var by_file: Dictionary = _problems[source]
	if files.is_empty():
		by_file.clear()
	else:
		for path in files:
			by_file.erase(path)
	for problem: Dictionary in problems:
		problem.source = source
		if not by_file.has(problem.file):
			by_file[problem.file] = []
		by_file[problem.file].append(problem)
	_rebuild()
	problems_changed.emit()


func _rebuild() -> void:
	if not is_instance_valid(_tree):
		return
	_tree.clear()
	var root := _tree.create_item()
	var files := {}
	for source: String in _problems:
		for path: String in _problems[source]:
			files[path] = true
	var paths := files.keys()
	paths.sort()
	var shown := 0
	for path: String in paths:
		if _scope.selected == SCOPE_SCRIPT and path != _current_script:
			continue
		var problems := problems_for(path).filter(_is_shown)
		if problems.is_empty():
			continue
		var file_item := _tree.create_item(root)
		file_item.set_text(0, path.trim_prefix("res://"))
		file_item.set_icon(0, _icon("GDScript"))
		var plural := "" if problems.size() == 1 else "s"
		file_item.set_text(1, "%d problem%s" % [problems.size(), plural])
		file_item.set_metadata(0, { "file": path, "line": 1, "column": 1 })
		for problem: Dictionary in problems:
			var item := _tree.create_item(file_item)
			var icon := "StatusError" if problem.severity == "error" else "StatusWarning"
			item.set_icon(0, _icon(icon))
			item.set_text(0, "line %d" % problem.line)
			item.set_text(1, problem.message)
			item.set_tooltip_text(1, problem.message)
			item.set_text(2, problem.rule)
			var source := "lint" if problem.source == SOURCE_LINT else "type check"
			item.set_tooltip_text(2, "%s (%s)" % [problem.rule, source])
			item.set_metadata(0, problem)
			shown += 1
	var count := counts()
	if count.errors + count.warnings == 0:
		_status.text = "No problems"
	else:
		_status.text = "%d error%s, %d warning%s" % [
			count.errors, "" if count.errors == 1 else "s",
			count.warnings, "" if count.warnings == 1 else "s",
		]


func _is_shown(problem: Dictionary) -> bool:
	if problem.severity == "error" and not _errors_button.button_pressed:
		return false
	if problem.severity != "error" and not _warnings_button.button_pressed:
		return false
	if problem.source == SOURCE_LINT and not _lint_button.button_pressed:
		return false
	if problem.source == SOURCE_TYPES and not _types_button.button_pressed:
		return false
	return true


func _on_item_activated() -> void:
	var item := _tree.get_selected()
	if item != null:
		action_requested.emit("open", item.get_metadata(0))


func _on_item_mouse_selected(_position: Vector2, button: int) -> void:
	if button != MOUSE_BUTTON_RIGHT:
		return
	var item := _tree.get_selected()
	if item == null or not (item.get_metadata(0) as Dictionary).has("rule"):
		return
	_context_problem = item.get_metadata(0)
	_context_menu.clear()
	_context_menu.add_item("Open", MENU_OPEN)
	if _context_problem.source == SOURCE_LINT:
		if _context_problem.fixable:
			_context_menu.add_item("Fix", MENU_FIX)
		_context_menu.add_item("Ignore on this line", MENU_IGNORE)
		var turn_off := 'Turn off "%s" for the project' % _context_problem.rule
		_context_menu.add_item(turn_off, MENU_DISABLE)
		_context_menu.add_item('What does "%s" mean?' % _context_problem.rule, MENU_DOCS)
	_context_menu.add_item("Copy the message", MENU_COPY)
	_context_menu.position = Vector2i(get_screen_position() + get_local_mouse_position())
	_context_menu.reset_size()
	_context_menu.popup()


func _on_context_menu(id: int) -> void:
	match id:
		MENU_OPEN:
			action_requested.emit("open", _context_problem)
		MENU_FIX:
			action_requested.emit("fix", _context_problem)
		MENU_IGNORE:
			action_requested.emit("ignore_here", _context_problem)
		MENU_DISABLE:
			action_requested.emit("disable_rule", _context_problem)
		MENU_DOCS:
			action_requested.emit("docs", _context_problem)
		MENU_COPY:
			DisplayServer.clipboard_set(_context_problem.message)


func _fill_settings_menu() -> void:
	var popup := _settings_menu.get_popup()
	popup.clear()
	for index in SETTING_IDS.size():
		var key: String = SETTING_IDS[index]
		var label: String = Settings.LABELS[key]
		if Settings.is_set_by_project(key):
			label += " (set by this project)"
		popup.add_check_item(label, index)
		popup.set_item_checked(index, bool(Settings.value(key)))
		popup.set_item_disabled(index, Settings.is_set_by_project(key))
	popup.add_separator()
	popup.add_item("Open gdsuite.toml", MENU_OPEN_CONFIG)


func _on_settings_menu(id: int) -> void:
	if id == MENU_OPEN_CONFIG:
		open_config_requested.emit()
		return
	var key: String = SETTING_IDS[id]
	var enabled := not bool(Settings.value(key))
	Settings.set_personal(key, enabled)
	setting_toggled.emit(key, enabled)


func _toggle(bar: HBoxContainer, text: String, icon_name: String, tooltip: String) -> Button:
	var button := Button.new()
	button.toggle_mode = true
	button.button_pressed = true
	button.flat = true
	button.text = text
	button.tooltip_text = tooltip
	if icon_name != "":
		button.icon = _icon(icon_name)
	button.toggled.connect(func(_on: bool) -> void: _rebuild())
	bar.add_child(button)
	return button


func _icon(icon_name: String) -> Texture2D:
	return EditorInterface.get_editor_theme().get_icon(icon_name, "EditorIcons")
