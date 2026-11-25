@tool
## The scripts open in the script editor: finding them, and changing their
## text. Nothing here holds state.


static func current_path() -> String:
	var script := EditorInterface.get_script_editor().get_current_script()
	return "" if script == null else script.resource_path


static func current_code_edit() -> CodeEdit:
	var editor := EditorInterface.get_script_editor().get_current_editor()
	if editor == null:
		return null
	return editor.get_base_editor() as CodeEdit


## The editor showing the script at `path`, or null when it isn't open.
static func code_edit_for(path: String) -> CodeEdit:
	var script_editor := EditorInterface.get_script_editor()
	var scripts := script_editor.get_open_scripts()
	var editors := script_editor.get_open_script_editors()
	if scripts.size() != editors.size():
		return null
	for index in scripts.size():
		if scripts[index].resource_path == path:
			return editors[index].get_base_editor() as CodeEdit
	return null


static func is_unsaved(path: String) -> bool:
	var code_edit := code_edit_for(path)
	return code_edit != null and code_edit.get_version() != code_edit.get_saved_version()


## Shows the script at `path`, changed on disk, as it is now.
static func reload_file(path: String) -> void:
	var text := FileAccess.get_file_as_string(path)
	if text == "" or not ResourceLoader.has_cached(path):
		return
	var script := load(path) as GDScript
	if script != null:
		script.source_code = text
	var code_edit := code_edit_for(path)
	if code_edit != null:
		replace_text(code_edit, text, true)


## Replaces the editor's text in one undo step, keeping the caret and the
## scroll position. `saved` marks the result as saved.
static func replace_text(code_edit: CodeEdit, text: String, saved: bool) -> void:
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


static func open(file: String, line := 1, column := 1) -> void:
	var script := load(file) as Script
	if script != null:
		EditorInterface.edit_script(script, maxi(line, 1), maxi(column, 1) - 1)


static func toast(message: String, warning := false) -> void:
	var toaster := EditorInterface.get_editor_toaster()
	var severity := EditorToaster.SEVERITY_WARNING if warning else EditorToaster.SEVERITY_INFO
	toaster.push_toast("GDSuite: " + message, severity)
