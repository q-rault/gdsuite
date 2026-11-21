@tool
extends RefCounted
## A ▶ in the script editor's gutter on each test function: click it to run
## that test. After a run, it shows the test's result.

const GUTTER_NAME := "gdsuite_tests"


## Marks the test functions of `tests` ([{name, status}]) in `code_edit`.
func show(code_edit: CodeEdit, tests: Array) -> void:
	var gutter := _gutter(code_edit)
	var theme := EditorInterface.get_editor_theme()
	var statuses := {}
	for test: Dictionary in tests:
		statuses[test.name] = test.status
	for line in code_edit.get_line_count():
		var test_name := _test_on(code_edit.get_line(line))
		if test_name == "" or not statuses.has(test_name):
			code_edit.set_line_gutter_icon(line, gutter, null)
			code_edit.set_line_gutter_clickable(line, gutter, false)
			code_edit.set_line_gutter_metadata(line, gutter, null)
			continue
		var icon := "Play"
		match statuses[test_name]:
			"pass":
				icon = "StatusSuccess"
			"fail", "error":
				icon = "StatusError"
			"running":
				icon = "Reload"
		code_edit.set_line_gutter_icon(line, gutter, theme.get_icon(icon, "EditorIcons"))
		code_edit.set_line_gutter_clickable(line, gutter, true)
		code_edit.set_line_gutter_metadata(line, gutter, test_name)


func clear(code_edit: CodeEdit) -> void:
	var gutter := _find_gutter(code_edit)
	if gutter < 0:
		return
	for line in code_edit.get_line_count():
		code_edit.set_line_gutter_icon(line, gutter, null)
		code_edit.set_line_gutter_clickable(line, gutter, false)
		code_edit.set_line_gutter_metadata(line, gutter, null)


## The test behind a click on the gutter, or "".
func test_at(code_edit: CodeEdit, line: int, gutter: int) -> String:
	if gutter < 0 or gutter != _find_gutter(code_edit):
		return ""
	var test_name: Variant = code_edit.get_line_gutter_metadata(line, gutter)
	return test_name if test_name is String else ""


## The line of `test_name`'s function in `code_edit`, from 0, or -1.
static func line_of(code_edit: CodeEdit, test_name: String) -> int:
	for line in code_edit.get_line_count():
		if _test_on(code_edit.get_line(line)) == test_name:
			return line
	return -1


## The test a line declares: "func test_jump(" gives "test_jump".
static func _test_on(text: String) -> String:
	var stripped := text.strip_edges()
	if not stripped.begins_with("func test_"):
		return ""
	return stripped.trim_prefix("func ").get_slice("(", 0).strip_edges()


func _gutter(code_edit: CodeEdit) -> int:
	var gutter := _find_gutter(code_edit)
	if gutter >= 0:
		return gutter
	# Last, after Godot's own gutters, as the problem markers.
	code_edit.add_gutter()
	gutter = code_edit.get_gutter_count() - 1
	code_edit.set_gutter_name(gutter, GUTTER_NAME)
	code_edit.set_gutter_type(gutter, TextEdit.GUTTER_TYPE_ICON)
	code_edit.set_gutter_width(gutter, code_edit.get_line_height())
	code_edit.set_gutter_clickable(gutter, true)
	code_edit.set_gutter_draw(gutter, true)
	return gutter


func _find_gutter(code_edit: CodeEdit) -> int:
	for gutter in code_edit.get_gutter_count():
		if code_edit.get_gutter_name(gutter) == GUTTER_NAME:
			return gutter
	return -1
