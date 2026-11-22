@tool
extends VBoxContainer
## The Problems dock: what formatting, lint and the compile check found, by
## script. Each script is a foldable row with its counts, then one line per
## problem:
##   × or ⚠  line:col  message  code · tool
## where the code names the problem (`max-line-length`, `unused-variable`) and
## the tool is the one that found it: format, lint or compile. The scripts
## that formatting would change are one row at the top. Below the list, the
## problem selected: all of its message, what its code means, and what can
## be done about it.

signal problems_changed
## A check started or ended, or a script was edited: whether the project is
## known to be clean (`is_clean`) may have changed.
signal state_changed
## Asks the plugin to run something on a problem: "open", "fix",
## "disable_rule", "ignore_here", "docs", or "format_all" with {files}.
signal action_requested(action: String, problem: Dictionary)
## Asks the plugin to look again: format check, lint and compile the project.
signal refresh_requested
signal setting_toggled(key: String, enabled: bool)
signal open_config_requested

const Settings = preload("settings.gd")
const ColumnResizer = preload("column_resizer.gd")
const Ui = preload("ui.gd")
const Problem = preload("problem.gd")
const DetailsPane = preload("details_pane.gd")
const Folding = preload("folding.gd")

## The columns, by name, in their order. The message is always shown: it is
## the one that takes the room the others leave.
const COLUMN_MARK := "mark"
const COLUMN_LINE := "line"
const COLUMN_MESSAGE := "message"
const COLUMN_SOURCE := "source"
const COLUMNS := [COLUMN_MARK, COLUMN_LINE, COLUMN_MESSAGE, COLUMN_SOURCE]
const COLUMN_TITLES := {
	COLUMN_MARK: "Error or warning mark",
	COLUMN_LINE: "Line",
	COLUMN_MESSAGE: "Message",
	COLUMN_SOURCE: "Code and tool",
}
## The columns the user can resize by dragging their edge.
const RESIZABLE := [COLUMN_LINE, COLUMN_SOURCE]
## Where the shown columns and their widths are kept, per project.
const VISIBLE_COLUMNS_KEY := "problems_columns"
const WIDTHS_KEY := "problems_widths"

## The share of the list's width the code and tool take at most when they
## size themselves: the rest is the message's.
const SOURCE_SHARE := 0.25
## What the details say when no problem is selected.
const NO_SELECTION := (
	"Select a problem to see all of it. Alt+F8 goes to the next problem, Shift+Alt+F8 to the "
	+ "one before."
)
## What a row of the list is: its "kind" meta.
const ROW_FILE := "file"
const ROW_PROBLEM := "problem"
const ROW_FORMAT := "format"

const BUTTON_COPY := 1

const MENU_OPEN := 0
const MENU_FIX := 1
const MENU_IGNORE := 2
const MENU_DISABLE := 3
const MENU_DOCS := 4
const MENU_COPY := 5
const MENU_FORMAT_ALL := 6
const MENU_OPEN_CONFIG := 100
## The Columns submenu: one check item per column, by its index in COLUMNS.
const MENU_RESET_WIDTHS := 200

const SETTING_IDS := [
	Settings.FORMAT_ON_SAVE,
	Settings.LINT_ON_SAVE,
	Settings.TYPE_CHECK_ON_SAVE,
	Settings.SHOW_IN_GUTTER,
]

## source -> { "res://path.gd": [problem, ...] }. A problem is a Dictionary:
## {file, line, column, severity, message, rule, fixable, source}.
var _problems := { Problem.SOURCE_FORMAT: {}, Problem.SOURCE_LINT: {}, Problem.SOURCE_TYPES: {} }
## The sources that reported at least once: until all have, the project's
## state is not known.
var _reported := {}
## source -> how many of its checks run now: its filter shows a spinner.
var _running := {}
## Scripts edited since they were checked: their problems may be gone.
var _stale := {}
## Scripts edited since they were checked, with problems or not.
var _unchecked := {}
## Scripts the user folded. Nothing else folds or unfolds them.
var _folded := {}
var _current_script := ""
## When a check last reported, in msec, for "Checked 3 min ago".
var _checked_msec := 0
## What went wrong with the last check, shown until the next one reports.
var _note := ""
var _spinner_frame := 0

var _tree: Tree
var _status: Label
var _errors_button: Button
var _warnings_button: Button
var _source_buttons := {}
## source -> [errors Label, warnings Label, spinner] on its filter.
var _source_counts := {}
var _current_only: Button
## Folds every script, or unfolds them all once all are folded.
var _fold_button: Button
var _check_button: Button
var _check_icon: Texture2D
var _settings_menu: MenuButton
var _columns_menu: PopupMenu
var _context_menu: PopupMenu
var _context_problem := {}
var _details: RichTextLabel
var _pane: DetailsPane
var _actions: HBoxContainer
## The shown columns, in order, and column name -> index in the tree.
var _shown: Array = []
var _index := {}
var _resizer := ColumnResizer.new()
## The row under the mouse: it shows the copy button.
var _hovered_item: TreeItem
var _spinner: Timer
var _clock: Timer


func _ready() -> void:
	name = "Problems"
	# The addon is in English: Godot would translate the words it knows.
	auto_translate_mode = AUTO_TRANSLATE_MODE_DISABLED
	size_flags_vertical = SIZE_EXPAND_FILL
	custom_minimum_size = Vector2(0, 120)

	# As in the Tests dock: what to show on the left, what to do on the right.
	var bar := Ui.toolbar()
	add_child(bar)
	_errors_button = Ui.counted_toggle(
		bar, Ui.icon("StatusError"), "", "Errors: show the errors", _rebuild
	)
	_warnings_button = Ui.counted_toggle(
		bar, Ui.icon("StatusWarning"), "", "Warnings: show the warnings", _rebuild
	)
	bar.add_child(VSeparator.new())
	_source_toggle(bar, Problem.SOURCE_FORMAT, "Format", "Scripts that formatting would change")
	_source_toggle(bar, Problem.SOURCE_LINT, "Lint", "What the linter found")
	_source_toggle(
		bar, Problem.SOURCE_TYPES, "Compile",
		"What Godot's compiler reports: GDScript and shader errors, Godot's warnings, "
		+ "missing files"
	)
	bar.add_child(VSeparator.new())
	# One of two: the whole project, or the script being edited.
	var scope := Ui.switch(
		bar,
		[
			["Project", "", "List the problems of every script"],
			["Current", "Script", "Only list the script you're editing"],
		],
		0,
		func(_index: int) -> void: _rebuild()
	)
	_current_only = scope[1]
	_fold_button = Ui.button(bar, "CollapseTree", "", "Fold every script", _fold_all)

	_status = Label.new()
	_status.size_flags_horizontal = SIZE_EXPAND_FILL
	_status.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_status.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	_status.modulate.a = Ui.FADED_ALPHA
	bar.add_child(_status)

	_check_button = Ui.button(
		bar, "", "Check", "Check the whole project again: format, lint and compile",
		func() -> void: refresh_requested.emit()
	)
	_check_button.icon = _check_icon

	_settings_menu = MenuButton.new()
	_settings_menu.flat = true
	_settings_menu.icon = Ui.icon("Tools")
	_settings_menu.tooltip_text = "GDSuite settings, and the columns to show"
	_settings_menu.about_to_popup.connect(_fill_settings_menu)
	_settings_menu.get_popup().id_pressed.connect(_on_settings_menu)
	_columns_menu = PopupMenu.new()
	_columns_menu.name = "Columns"
	_columns_menu.hide_on_checkable_item_selection = false
	_columns_menu.id_pressed.connect(_on_columns_menu)
	_settings_menu.get_popup().add_child(_columns_menu)
	bar.add_child(_settings_menu)

	_tree = Tree.new()
	_tree.size_flags_vertical = SIZE_EXPAND_FILL
	_tree.hide_root = true
	_tree.select_mode = Tree.SELECT_ROW
	Ui.style_tree(_tree)
	_tree.resized.connect(_layout)
	_tree.allow_rmb_select = true
	_tree.item_selected.connect(_show_details)
	_tree.item_activated.connect(_on_item_activated)
	_tree.item_mouse_selected.connect(_on_item_mouse_selected)
	_tree.item_collapsed.connect(_on_item_collapsed)
	_tree.button_clicked.connect(_on_button_clicked)
	_tree.empty_clicked.connect(func(_p: Vector2, _b: int) -> void: _deselect())
	_tree.gui_input.connect(_on_tree_input)
	_tree.mouse_exited.connect(func() -> void: _hover(null))
	_resizer.resized.connect(func(_column: int) -> void: _layout())

	# The problem selected, as in the Tests dock: all of it, then what can be
	# done about it.
	var details := VBoxContainer.new()
	_details = RichTextLabel.new()
	_details.bbcode_enabled = true
	_details.selection_enabled = true
	_details.size_flags_vertical = SIZE_EXPAND_FILL
	_details.add_theme_font_size_override("normal_font_size", Ui.font_size())
	_details.add_theme_font_size_override("bold_font_size", Ui.font_size())
	_details.meta_clicked.connect(func(_meta: Variant) -> void: _on_action(MENU_DOCS))
	details.add_child(_details)
	_actions = HBoxContainer.new()
	details.add_child(_actions)
	_pane = DetailsPane.new(_tree, details, _details, "problems_details")
	add_child(_pane)

	_context_menu = PopupMenu.new()
	_context_menu.id_pressed.connect(_on_action)
	add_child(_context_menu)

	_spinner = Timer.new()
	_spinner.wait_time = 0.1
	_spinner.timeout.connect(_on_spinner)
	add_child(_spinner)
	_clock = Timer.new()
	_clock.wait_time = 5.0
	_clock.timeout.connect(_update_status)
	add_child(_clock)
	_clock.start()

	_load_columns()
	_rebuild()


# --- What the plugin calls ----------------------------------------------------


## Replaces the lint findings of `files` ("res://" paths), or of the whole
## project when `files` is empty.
func set_lint(files: PackedStringArray, problems: Array) -> void:
	_replace(Problem.SOURCE_LINT, files, problems)


## Replaces every type check finding: the type check covers the project.
func set_types(problems: Array) -> void:
	_replace(Problem.SOURCE_TYPES, PackedStringArray(), problems)


## Replaces the format findings of `files`, or of the whole project.
func set_format(files: PackedStringArray, problems: Array) -> void:
	_replace(Problem.SOURCE_FORMAT, files, problems)


## A tool started or ended a check: its filter shows a spinner meanwhile.
func set_running(source: String, running: bool) -> void:
	var count: int = _running.get(source, 0) + (1 if running else -1)
	if count > 0:
		_running[source] = count
	else:
		_running.erase(source)
	if not is_instance_valid(_spinner):
		return
	if _running.is_empty():
		_spinner.stop()
	elif _spinner.is_stopped():
		_spinner.start()
	_on_spinner()
	_update_status()
	state_changed.emit()


## The script at `path` was edited since it was checked: its problems are
## faded until a check reports on it again.
func mark_stale(path: String) -> void:
	if not _unchecked.has(path):
		_unchecked[path] = true
		state_changed.emit()
	if _stale.has(path) or not _has_problems(path):
		return
	_stale[path] = true
	_rebuild()


## The Check button's icon: the plugin's, as on the Check icon of the
## editor's top bar.
func set_check_icon(icon: Texture2D) -> void:
	_check_icon = icon
	if is_instance_valid(_check_button):
		_check_button.icon = icon


## Whether format, lint and the type check have each reported once.
func is_known() -> bool:
	return _reported.size() == _problems.size()


## Whether the project has no problem, as far as the checks of what every
## script holds now can tell: all of them reported, none is running, and no
## script was edited since.
func is_clean() -> bool:
	var count := counts()
	return (
		is_known() and _running.is_empty() and _unchecked.is_empty()
		and count.errors + count.warnings == 0
	)


## Says what went wrong with a check, until the next one reports.
func set_status(text: String) -> void:
	_note = text
	_update_status()


func set_current_script(path: String) -> void:
	_current_script = path
	if is_instance_valid(_tree):
		_rebuild()


## The problems of one script, every source, sorted by line.
func problems_for(path: String) -> Array:
	var found := []
	for source: String in _problems:
		found.append_array(_problems[source].get(path, []))
	found.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return a.line < b.line)
	return found


## {errors, warnings} over the whole project, of one source or of all.
func counts(source := "") -> Dictionary:
	var errors := 0
	var warnings := 0
	for each: String in _problems:
		if source != "" and each != source:
			continue
		for path: String in _problems[each]:
			for problem: Dictionary in _problems[each][path]:
				if problem.severity == "error":
					errors += 1
				else:
					warnings += 1
	return { "errors": errors, "warnings": warnings }


## The problem after (`direction` 1) or before (-1) line `line` of `path`,
## in the list's order and with its filters, around the end; selected in the
## list. Empty when the list has none. Format rows are left out: they have
## no line to go to.
func step(path: String, line: int, direction: int) -> Dictionary:
	var rows := _problem_rows()
	if rows.is_empty():
		return {}
	var order := {}
	for item: TreeItem in rows:
		var file: String = item.get_metadata(0).file
		if not order.has(file):
			order[file] = order.size()
	# Where the caret is, in the list's order: a script not listed is before
	# every other.
	var here := Vector2i(order.get(path, -1), line)
	var found: TreeItem = null
	if direction > 0:
		for item: TreeItem in rows:
			if _position(item, order) > here:
				found = item
				break
		if found == null:
			found = rows[0]
	else:
		for index in range(rows.size() - 1, -1, -1):
			if _position(rows[index], order) < here:
				found = rows[index]
				break
		if found == null:
			found = rows[-1]
	var parent := found.get_parent()
	if parent.collapsed:
		parent.collapsed = false
	found.select(0)
	_tree.scroll_to_item(found)
	return found.get_metadata(0)


func _position(item: TreeItem, order: Dictionary) -> Vector2i:
	var problem: Dictionary = item.get_metadata(0)
	return Vector2i(order[problem.file], int(problem.line))


func _problem_rows() -> Array:
	var rows := []
	if _tree.get_root() == null:
		return rows
	for file_item in _tree.get_root().get_children():
		if file_item.get_meta("kind") != ROW_FILE:
			continue
		for item in file_item.get_children():
			rows.append(item)
	return rows


func _has_problems(path: String) -> bool:
	return _problems.values().any(func(by_file: Dictionary) -> bool: return by_file.has(path))


func _replace(source: String, files: PackedStringArray, problems: Array) -> void:
	_checked_msec = Time.get_ticks_msec()
	_note = ""
	var by_file: Dictionary = _problems[source]
	if files.is_empty():
		_reported[source] = true
		by_file.clear()
		_stale.clear()
		_unchecked.clear()
	else:
		for path in files:
			by_file.erase(path)
			_stale.erase(path)
			_unchecked.erase(path)
	for problem: Dictionary in problems:
		problem.source = source
		if not by_file.has(problem.file):
			by_file[problem.file] = []
		by_file[problem.file].append(problem)
	_rebuild()
	problems_changed.emit()


# --- Columns -----------------------------------------------------------------


func _load_columns() -> void:
	var saved: Variant = EditorInterface.get_editor_settings().get_project_metadata(
		"gdsuite", VISIBLE_COLUMNS_KEY, COLUMNS
	)
	_shown = []
	for column: String in COLUMNS:
		if column == COLUMN_MESSAGE or (saved is Array and column in saved):
			_shown.append(column)
	_apply_columns()


## Gives the tree the shown columns, and makes the fixed ones resizable.
func _apply_columns() -> void:
	_index.clear()
	_tree.columns = _shown.size()
	var defaults := {}
	var names := {}
	for index in _shown.size():
		var column: String = _shown[index]
		_index[column] = index
		_tree.set_column_expand(index, column == COLUMN_MESSAGE)
		_tree.set_column_clip_content(index, true)
		if column in RESIZABLE:
			defaults[index] = 80
			names[index] = column
	_resizer.attach(_tree, WIDTHS_KEY, defaults, names)


func _set_column_shown(column: String, shown: bool) -> void:
	if column == COLUMN_MESSAGE:
		return
	if shown and column not in _shown:
		_shown.append(column)
		_shown.sort_custom(
			func(a: String, b: String) -> bool: return COLUMNS.find(a) < COLUMNS.find(b)
		)
	elif not shown:
		_shown.erase(column)
	EditorInterface.get_editor_settings().set_project_metadata(
		"gdsuite", VISIBLE_COLUMNS_KEY, _shown
	)
	_apply_columns()
	_rebuild()


## Sets a cell, when its column is shown.
func _cell(item: TreeItem, column: String, text: String, tooltip := "") -> void:
	if not _index.has(column):
		return
	var index: int = _index[column]
	item.set_text(index, text)
	if tooltip != "":
		item.set_tooltip_text(index, tooltip)


# --- The list ----------------------------------------------------------------


func _rebuild() -> void:
	if not is_instance_valid(_tree):
		return
	var selected := _selection_key(_tree.get_selected())
	_tree.clear()
	_hovered_item = null
	var root := _tree.create_item()
	var reselect: TreeItem = null

	var unformatted := _unformatted()
	if not unformatted.is_empty():
		var item := _format_row(root, unformatted)
		if selected == _selection_key(item):
			reselect = item

	for path: String in _listed_paths():
		var problems := problems_for(path).filter(
			func(p: Dictionary) -> bool: return p.source != Problem.SOURCE_FORMAT and _is_shown(p)
		)
		if problems.is_empty():
			continue
		var file_item := _file_row(root, path, problems, path in unformatted)
		if selected == _selection_key(file_item):
			reselect = file_item
		for problem: Dictionary in problems:
			var item := _problem_row(file_item, problem)
			if selected == _selection_key(item):
				reselect = item
	if reselect != null:
		reselect.select(0)
	_layout()
	_update_counts()
	_update_status()
	_show_details()
	_update_fold_button()


## The scripts listed, in order: those with errors, then the others, each by
## name. The script being edited is not moved up: stepping through the problems
## opens other scripts, and the order has to stay put while it does.
func _listed_paths() -> Array:
	var files := {}
	for source: String in _problems:
		for path: String in _problems[source]:
			if not _current_only.button_pressed or path == _current_script:
				files[path] = true
	var paths := files.keys()
	var rank := func(path: String) -> int:
		var has_error := problems_for(path).any(
			func(p: Dictionary) -> bool: return p.severity == "error"
		)
		return 0 if has_error else 1
	paths.sort_custom(
		func(a: String, b: String) -> bool:
			var rank_a: int = rank.call(a)
			var rank_b: int = rank.call(b)
			return rank_a < rank_b if rank_a != rank_b else a < b
	)
	return paths


## The scripts not formatted that the filters let through.
func _unformatted() -> Array:
	var shown: bool = _source_buttons[Problem.SOURCE_FORMAT].button_pressed
	if not shown or not _warnings_button.button_pressed:
		return []
	var paths := []
	for path: String in _problems[Problem.SOURCE_FORMAT]:
		if not _current_only.button_pressed or path == _current_script:
			paths.append(path)
	paths.sort()
	return paths


## One row for every script formatting would change: one action fixes them.
func _format_row(root: TreeItem, paths: Array) -> TreeItem:
	var item := _tree.create_item(root)
	item.set_meta("kind", ROW_FORMAT)
	item.set_metadata(0, { "files": paths, "severity": "warning", "source": Problem.SOURCE_FORMAT })
	var one := paths.size() == 1 and _current_only.button_pressed
	var text := "This script is not formatted" if one \
			else "%d script%s not formatted" % [paths.size(), "" if paths.size() == 1 else "s"]
	item.set_text(0, "%s  %s   ·   double-click to format %s" % [
		Problem.WARNING_MARK, text, "it" if paths.size() == 1 else "them all",
	])
	item.set_expand_right(0, true)
	var names := paths.map(func(p: String) -> String: return p.trim_prefix("res://"))
	item.set_tooltip_text(0, "\n".join(names))
	item.set_custom_color(0, Problem.severity_color("warning"))
	return item


## A script's row: its path and what it has, foldable.
func _file_row(root: TreeItem, path: String, problems: Array, unformatted: bool) -> TreeItem:
	var item := _tree.create_item(root)
	item.set_meta("kind", ROW_FILE)
	item.set_metadata(0, { "file": path, "line": 0, "severity": "", "source": "" })
	var errors := problems.filter(func(p: Dictionary) -> bool: return p.severity == "error").size()
	var warnings := problems.size() - errors
	var parts := PackedStringArray()
	if errors > 0:
		parts.append("%d %s" % [errors, Problem.ERROR_MARK])
	if warnings > 0:
		parts.append("%d %s" % [warnings, Problem.WARNING_MARK])
	if unformatted:
		parts.append("not formatted")
	if _stale.has(path):
		parts.append("edited since checked")
	item.set_text(0, "%s    %s" % [path.trim_prefix("res://"), "  ·  ".join(parts)])
	item.set_expand_right(0, true)
	item.set_tooltip_text(0, path)
	var color := Ui.text_color()
	item.set_custom_color(0, Ui.faded(color) if _stale.has(path) else color)
	item.collapsed = _folded.has(path)
	return item


func _problem_row(parent: TreeItem, problem: Dictionary) -> TreeItem:
	var item := _tree.create_item(parent)
	item.set_meta("kind", ROW_PROBLEM)
	var is_error: bool = problem.severity == "error"
	item.set_metadata(0, problem)
	_cell(item, COLUMN_MARK, Problem.ERROR_MARK if is_error else Problem.WARNING_MARK,
			"Error" if is_error else "Warning")
	if _index.has(COLUMN_MARK):
		item.set_text_alignment(_index[COLUMN_MARK], HORIZONTAL_ALIGNMENT_CENTER)
		if is_error:
			var size := _tree.get_theme_font_size("font_size")
			item.set_custom_font_size(_index[COLUMN_MARK], roundi(size * Problem.ERROR_MARK_SCALE))
	_cell(item, COLUMN_LINE, Problem.line(problem), Problem.where(problem))
	if _index.has(COLUMN_LINE):
		item.set_text_alignment(_index[COLUMN_LINE], HORIZONTAL_ALIGNMENT_RIGHT)
	_cell(item, COLUMN_MESSAGE, problem.message, problem.message)
	_cell(item, COLUMN_SOURCE, Problem.source_text(problem), Problem.code_tooltip(problem))
	if _index.has(COLUMN_SOURCE):
		item.set_text_alignment(_index[COLUMN_SOURCE], HORIZONTAL_ALIGNMENT_RIGHT)
	var color := Problem.severity_color(problem.severity)
	if _stale.has(problem.file):
		color = Ui.faded(color)
	for column: String in _shown:
		item.set_custom_color(_index[column], color)
	return item


## What identifies a row across rebuilds, to keep it selected.
func _selection_key(item: TreeItem) -> String:
	if item == null:
		return ""
	var meta: Dictionary = item.get_metadata(0)
	match item.get_meta("kind"):
		ROW_FORMAT:
			return "format"
		ROW_FILE:
			return meta.file
	return "%s:%d:%s:%s" % [meta.file, meta.line, meta.rule, meta.message]


## Sizes the columns: to the width the user dragged them to, or else to what
## they hold. The code and tool take at most their share: the message keeps
## the rest.
func _layout() -> void:
	if not is_instance_valid(_tree) or _tree.get_root() == null:
		return
	var font := _tree.get_theme_font("font")
	var font_size := _tree.get_theme_font_size("font_size")
	var scale := EditorInterface.get_editor_scale()
	# Tree keeps part of a column for its own margins.
	var padding := 20 * scale
	var width := maxf(_tree.size.x, 400 * scale)
	var widths := {}
	for column: String in _shown:
		if column != COLUMN_MESSAGE:
			widths[column] = 0.0
	for item: TreeItem in _problem_rows():
		for column: String in widths:
			var text := item.get_text(_index[column])
			var size := font_size
			if column == COLUMN_MARK and item.get_metadata(0).severity == "error":
				size = roundi(font_size * Problem.ERROR_MARK_SCALE)
			widths[column] = maxf(widths[column], _text_width(text, font, size))
	if widths.has(COLUMN_SOURCE):
		widths[COLUMN_SOURCE] = minf(widths[COLUMN_SOURCE], width * SOURCE_SHARE - padding)
	for column: String in widths:
		var index: int = _index[column]
		var fitted := int(widths[column] + padding)
		if index == 0:
			# The first column holds the problems' indent under their script.
			fitted += _tree.get_theme_constant("item_margin")
		if column == _last_column():
			# Room for the copy button that shows on the hovered row.
			fitted += int(_copy_icon().get_width() + 8 * scale)
		var saved := _resizer.saved_width(index) if column in RESIZABLE else -1
		_tree.set_column_custom_minimum_width(index, saved if saved > 0 else fitted)


static func _text_width(text: String, font: Font, font_size: int) -> float:
	return font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x


## The problem as one line of text, as a compiler prints it:
## `path:line:col: error[code]: message (tool)`.
static func _line_text(problem: Dictionary) -> String:
	return "%s: %s[%s]: %s (%s)" % [
		Problem.where(problem), problem.severity, problem.rule, problem.message,
		Problem.TOOL_NAMES[problem.source],
	]


func _is_shown(problem: Dictionary) -> bool:
	if problem.severity == "error" and not _errors_button.button_pressed:
		return false
	if problem.severity != "error" and not _warnings_button.button_pressed:
		return false
	return _source_buttons[problem.source].button_pressed


# --- Hover, selection, details -------------------------------------------------


## Shows the copy button on `item`, the problem under the mouse, and takes
## it off the row before.
func _hover(item: TreeItem) -> void:
	if item != null and item.get_meta("kind") != ROW_PROBLEM:
		item = null
	if item == _hovered_item:
		return
	var column := _index.get(_last_column(), -1) as int
	if is_instance_valid(_hovered_item) and column >= 0:
		var button := _hovered_item.get_button_by_id(column, BUTTON_COPY)
		if button >= 0:
			_hovered_item.erase_button(column, button)
	_hovered_item = item
	if item != null and column >= 0:
		var color := Problem.severity_color(item.get_metadata(0).severity)
		item.add_button(column, _copy_icon(), BUTTON_COPY, false, "Copy this line")
		item.set_button_color(column, item.get_button_by_id(column, BUTTON_COPY), color)


func _last_column() -> String:
	return _shown[-1] if not _shown.is_empty() else COLUMN_MESSAGE


func _copy_icon() -> Texture2D:
	return Ui.icon("ActionCopy")


func _copy(problem: Dictionary) -> void:
	DisplayServer.clipboard_set(_line_text(problem))
	_note = "Copied"
	_update_status()


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


func _on_button_clicked(item: TreeItem, _column: int, id: int, _mouse_button: int) -> void:
	if id == BUTTON_COPY:
		_copy(item.get_metadata(0))


func _on_item_collapsed(item: TreeItem) -> void:
	_update_fold_button.call_deferred()
	if item.get_meta("kind", "") != ROW_FILE:
		return
	var path: String = item.get_metadata(0).file
	if item.collapsed:
		_folded[path] = true
	else:
		_folded.erase(path)


func _fold_all() -> void:
	Folding.toggle_all(_scripts())
	_update_fold_button()


func _update_fold_button() -> void:
	Folding.show_state(_fold_button, _scripts())


## The rows of the scripts listed.
func _scripts() -> Array:
	return _tree.get_root().get_children().filter(
		func(item: TreeItem) -> bool: return item.get_meta("kind") == ROW_FILE
	)


func _on_item_activated() -> void:
	var item := _tree.get_selected()
	if item == null:
		return
	if item.get_meta("kind") == ROW_FORMAT:
		action_requested.emit("format_all", item.get_metadata(0))
	else:
		action_requested.emit("open", item.get_metadata(0))


## What can be done about the selected row: [[label, menu id], ...].
func _actions_for(item: TreeItem) -> Array:
	var problem: Dictionary = item.get_metadata(0)
	match item.get_meta("kind"):
		ROW_FORMAT:
			var count: int = problem.files.size()
			var label := "Format the script" if count == 1 else "Format all %d" % count
			return [[label, MENU_FORMAT_ALL]]
		ROW_FILE:
			return [["Open", MENU_OPEN]]
	var actions := [["Open", MENU_OPEN]]
	if problem.source == Problem.SOURCE_LINT:
		if problem.fixable:
			actions.append(["Fix", MENU_FIX])
		actions.append(["Ignore on this line", MENU_IGNORE])
		actions.append(['Turn off "%s"' % problem.rule, MENU_DISABLE])
		actions.append(["What does it mean?", MENU_DOCS])
	actions.append(["Copy", MENU_COPY])
	return actions


## Below the list: all of the selected problem, what its code means, and the
## actions, as buttons.
func _show_details() -> void:
	if not is_instance_valid(_details):
		return
	for child in _actions.get_children():
		child.queue_free()
	var item := _tree.get_selected()
	if item == null:
		var dim := Ui.faded(Ui.text_color()).to_html()
		_details.text = "[color=#%s]%s[/color]" % [dim, NO_SELECTION]
		_pane.refresh_summary()
		return
	var problem: Dictionary = item.get_metadata(0)
	var dim := Ui.faded(Ui.text_color()).to_html()
	match item.get_meta("kind"):
		ROW_FORMAT:
			var lines := PackedStringArray()
			for path: String in problem.files:
				var first: Dictionary = _problems[Problem.SOURCE_FORMAT][path][0]
				lines.append("%s  [color=#%s]%s[/color]" % [
					Problem.escape(path.trim_prefix("res://")), dim, Problem.escape(first.message),
				])
			_details.text = "\n".join(lines)
		ROW_FILE:
			_details.text = "%s\n[color=#%s]Double-click to open it.[/color]" % [
				Problem.escape(problem.file), dim,
			]
		_:
			_details.text = Problem.details(problem)
	_context_problem = problem
	_pane.refresh_summary()
	for action: Array in _actions_for(item):
		Ui.button(_actions, "", action[0], "", _on_action.bind(action[1]))


func _on_item_mouse_selected(_position: Vector2, button: int) -> void:
	if button != MOUSE_BUTTON_RIGHT:
		return
	var item := _tree.get_selected()
	if item == null:
		return
	_context_problem = item.get_metadata(0)
	_context_menu.clear()
	for action: Array in _actions_for(item):
		_context_menu.add_item(action[0], action[1])
	_context_menu.position = Vector2i(get_screen_position() + get_local_mouse_position())
	_context_menu.reset_size()
	_context_menu.popup()


func _on_action(id: int) -> void:
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
			if _context_problem.get("source") == Problem.SOURCE_LINT:
				action_requested.emit("docs", _context_problem)
		MENU_COPY:
			_copy(_context_problem)
		MENU_FORMAT_ALL:
			action_requested.emit("format_all", _context_problem)


# --- The bar -----------------------------------------------------------------


## The counts: errors in red and warnings in yellow, on the Errors and
## Warnings filters, and both on each tool's.
func _update_counts() -> void:
	var total := counts()
	Ui.show_count(_errors_button.get_meta("count"), total.errors, Problem.severity_color("error"))
	var yellow := Problem.severity_color("warning")
	Ui.show_count(_warnings_button.get_meta("count"), total.warnings, yellow)
	for source: String in _source_counts:
		var count := counts(source)
		var labels: Array = _source_counts[source]
		Ui.show_count(labels[0], count.errors, Problem.severity_color("error"))
		Ui.show_count(labels[1], count.warnings, Problem.severity_color("warning"))
		var button: Button = _source_buttons[source]
		button.tooltip_text = "%s\n%d error%s, %d warning%s" % [
			button.get_meta("description"),
			count.errors, "" if count.errors == 1 else "s",
			count.warnings, "" if count.warnings == 1 else "s",
		]


## "Checking…", what went wrong, or how long ago the last check reported.
func _update_status() -> void:
	if is_instance_valid(_status):
		var checking := not _running.is_empty()
		Ui.set_status(_status, _note if _note != "" else Problem.status(checking, _checked_msec))


func _on_spinner() -> void:
	_spinner_frame += 1
	for source: String in _source_counts:
		Ui.spin(_source_counts[source][2], _running.has(source), _spinner_frame)


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
	_columns_menu.clear()
	for index in COLUMNS.size():
		var column: String = COLUMNS[index]
		_columns_menu.add_check_item(COLUMN_TITLES[column], index)
		_columns_menu.set_item_checked(index, column in _shown)
		_columns_menu.set_item_disabled(index, column == COLUMN_MESSAGE)
	_columns_menu.add_separator()
	_columns_menu.add_item("Reset column widths", MENU_RESET_WIDTHS)
	popup.add_submenu_node_item("Columns", _columns_menu)
	popup.add_separator()
	popup.add_item("Open gdsuite.toml", MENU_OPEN_CONFIG)


func _on_settings_menu(id: int) -> void:
	if id == MENU_OPEN_CONFIG:
		open_config_requested.emit()
		return
	if id < 0 or id >= SETTING_IDS.size():
		return
	var key: String = SETTING_IDS[id]
	var enabled := not bool(Settings.value(key))
	Settings.set_personal(key, enabled)
	setting_toggled.emit(key, enabled)


func _on_columns_menu(id: int) -> void:
	if id == MENU_RESET_WIDTHS:
		_resizer.reset()
		_layout()
		return
	var column: String = COLUMNS[id]
	var shown := column not in _shown
	_columns_menu.set_item_checked(id, shown)
	_set_column_shown(column, shown)


## A tool's filter: a spinner while it checks, its name, then its errors
## and its warnings.
func _source_toggle(bar: Container, source: String, text: String, tooltip: String) -> void:
	var button := Ui.counted_toggle(bar, null, text, tooltip, _rebuild)
	button.set_meta("description", tooltip)
	_source_buttons[source] = button
	var errors: Label = button.get_meta("count")
	_source_counts[source] = [errors, Ui.add_count(button), Ui.add_spinner(button)]
