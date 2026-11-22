@tool
extends EditorContextMenuPlugin
## "Run Tests" in the FileSystem dock's context menu, on folders and scripts.

## Called with the selected "res://" paths.
var run_paths: Callable


func _popup_menu(paths: PackedStringArray) -> void:
	for path in paths:
		if path.ends_with("/") or path.ends_with(".gd"):
			var icon := EditorInterface.get_editor_theme().get_icon("Play", "EditorIcons")
			add_context_menu_item("Run Tests", _on_run, icon)
			return


func _on_run(paths: Array) -> void:
	var targets := PackedStringArray()
	for path: String in paths:
		if path.ends_with("/") or path.ends_with(".gd"):
			targets.append(path)
	if not targets.is_empty() and run_paths.is_valid():
		run_paths.call(targets)
