@tool
extends RefCounted
## Marks problems in the script editor: a red × or a yellow ⚠ in a gutter of
## its own, as in the Problems dock, and a light tint of the same colour on
## the line. Click the mark to read the problem.

const Problem = preload("problem.gd")
const GUTTER_NAME := "gdsuite_problems"

## Lines this addon tinted, per CodeEdit, so it never clears Godot's own marks.
var _tinted := {}


## Shows `problems` (of the script `code_edit` holds) in `code_edit`.
func show(code_edit: CodeEdit, problems: Array) -> void:
	var gutter := _gutter(code_edit)
	var error_color := Problem.severity_color("error")
	var warning_color := Problem.severity_color("warning")

	clear(code_edit)
	var tinted: Array[int] = []
	var by_line := {}
	for problem: Dictionary in problems:
		var line: int = int(problem.line) - 1
		if line < 0 or line >= code_edit.get_line_count():
			continue
		if not by_line.has(line):
			by_line[line] = []
		by_line[line].append(problem)
	for line: int in by_line:
		var on_line: Array = by_line[line]
		var is_error := on_line.any(func(p: Dictionary) -> bool: return p.severity == "error")
		var only_info := on_line.all(func(p: Dictionary) -> bool: return p.severity == "info")
		var severity := "error" if is_error else "info" if only_info else "warning"
		code_edit.set_line_gutter_text(line, gutter, Problem.mark(severity))
		var color := error_color if is_error else warning_color
		if only_info:
			color = Problem.severity_color("info")
		code_edit.set_line_gutter_item_color(line, gutter, color)
		code_edit.set_line_gutter_clickable(line, gutter, true)
		code_edit.set_line_gutter_metadata(line, gutter, on_line)
		# Code health is about a function, or a script: its first line is no
		# more wrong than the others.
		if on_line.all(func(p: Dictionary) -> bool: return p.source == Problem.SOURCE_HEALTH):
			continue
		color.a = Problem.TINT_ALPHA
		code_edit.set_line_background_color(line, color)
		tinted.append(line)
	_tinted[code_edit.get_instance_id()] = tinted


## Removes this addon's marks from `code_edit`.
func clear(code_edit: CodeEdit) -> void:
	var gutter := _find_gutter(code_edit)
	if gutter >= 0:
		for line in code_edit.get_line_count():
			code_edit.set_line_gutter_text(line, gutter, "")
			code_edit.set_line_gutter_clickable(line, gutter, false)
			code_edit.set_line_gutter_metadata(line, gutter, null)
	for line: int in _tinted.get(code_edit.get_instance_id(), []):
		if line < code_edit.get_line_count():
			code_edit.set_line_background_color(line, Color(0, 0, 0, 0))
	_tinted.erase(code_edit.get_instance_id())


## The problems behind a click on the gutter, or an empty list.
func problems_at(code_edit: CodeEdit, line: int, gutter: int) -> Array:
	if gutter != _find_gutter(code_edit):
		return []
	var problems: Variant = code_edit.get_line_gutter_metadata(line, gutter)
	return problems if problems is Array else []


func _gutter(code_edit: CodeEdit) -> int:
	var gutter := _find_gutter(code_edit)
	if gutter >= 0:
		# An older version of the addon drew icons in it.
		code_edit.set_gutter_type(gutter, TextEdit.GUTTER_TYPE_STRING)
		return gutter
	# Last, after Godot's own gutters: inserting before them would move the
	# gutters the script editor keeps track of.
	code_edit.add_gutter()
	gutter = code_edit.get_gutter_count() - 1
	code_edit.set_gutter_name(gutter, GUTTER_NAME)
	code_edit.set_gutter_type(gutter, TextEdit.GUTTER_TYPE_STRING)
	code_edit.set_gutter_width(gutter, code_edit.get_line_height())
	code_edit.set_gutter_clickable(gutter, true)
	code_edit.set_gutter_draw(gutter, true)
	return gutter


func _find_gutter(code_edit: CodeEdit) -> int:
	for gutter in code_edit.get_gutter_count():
		if code_edit.get_gutter_name(gutter) == GUTTER_NAME:
			return gutter
	return -1
