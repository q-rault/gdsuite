@tool
extends RefCounted
## Finds the files of the project changed on disk by something other than
## the editor: a branch checked out, a pull, a script edited in another
## editor. What was checked or tested before is out of date then, as after a
## save, and nothing in the editor says so.
##
## It keeps when each file was last changed, as far as it knows, and compares
## once the editor has scanned the file system again. The editor's saves and
## the addon's own fixes are noted as they happen, so they don't count.

## The files that may change what a check or a test finds: scripts, scenes,
## resources, shaders. Not the reports the tools write, nor imported assets.
const EXTENSIONS := ["gd", "tscn", "tres", "scn", "res", "gdshader"]
## Files outside the editor's file system that change what every test runs
## with, and what the compile check finds.
const PROJECT_FILES := ["res://project.godot"]

## "res://path" -> when it was last changed, as FileAccess tells.
var _known := {}
var _scanned := false


## The files changed, added or removed since the last scan, and remembers
## them as they are now. The first scan only remembers.
func scan() -> PackedStringArray:
	var now := {}
	var root := EditorInterface.get_resource_filesystem().get_filesystem()
	if root != null:
		_collect(root, now)
	for path: String in PROJECT_FILES:
		if FileAccess.file_exists(path):
			now[path] = FileAccess.get_modified_time(path)
	var changed := PackedStringArray()
	if _scanned:
		for path: String in now:
			if _known.get(path, -1) != now[path]:
				changed.append(path)
		for path: String in _known:
			if not now.has(path):
				changed.append(path)
	_known = now
	_scanned = true
	return changed


## `path` was written by the editor or the addon: not a change from outside.
func note(path: String) -> void:
	if FileAccess.file_exists(path):
		_known[path] = FileAccess.get_modified_time(path)


func _collect(directory: EditorFileSystemDirectory, into: Dictionary) -> void:
	for index in directory.get_file_count():
		var path := directory.get_file_path(index)
		if path.get_extension() in EXTENSIONS:
			into[path] = FileAccess.get_modified_time(path)
	for index in directory.get_subdir_count():
		_collect(directory.get_subdir(index), into)
