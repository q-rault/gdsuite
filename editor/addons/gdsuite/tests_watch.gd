@tool
extends Node
## On save, for the Tests dock: a saved file makes the tests it can affect out
## of date, and with On save on, runs them again. gdtest says which tests: those
## that use the file, directly or through other scripts, scenes and resources.
##
## It doesn't run tests for nothing: it waits for the saves to stop, holds
## the tests of a script that doesn't compile, runs nothing for a script saved
## back to what its tests ran with, and a save during a run restarts it rather
## than queue another.

const Runner = preload("runner.gd")
const Result = preload("test_result.gd")

## Seconds without a save before it asks gdtest what the saves affect.
const SAVE_DELAY := 0.4

## Whether saving a file runs the tests it affects again.
var enabled := false
## Says whether a saved script compiles: "clean", "errors", or "pending"
## while the compile check has yet to report. Set by the plugin; a run waits
## for "clean".
var compile_state: Callable

## The Tests dock.
var _dock: Node
## The files saved in the last moments: it waits for the saves to stop
## (`_debounce`), so that several saves in a row make one run.
var _saves := {}
var _debounce: Timer
## Saved files whose tests run once they compile: path -> found, as
## `_on_affected` gets it.
var _waiting := {}
## path -> the MD5 of the content the tests last ran with. A file saved back
## to it needs no run.
var _ran_with := {}
## path -> the tests ("file::name") its last save made stale.
var _stale_by := {}
## path -> [MD5, stale tests] of the saved files the run going covers: once
## it ends, uninterrupted, they are what the tests ran with.
var _run_covers := {}


func _init(dock: Node) -> void:
	_dock = dock
	_debounce = Timer.new()
	_debounce.wait_time = SAVE_DELAY
	_debounce.one_shot = true
	_debounce.timeout.connect(_on_saves_settled)
	add_child(_debounce)


## A file of the project was saved: once the saves stop, finds the tests it
## affects.
func file_saved(path: String) -> void:
	if Runner.gdtest_program() == "":
		return
	# Back to what the tests last ran with, as after renaming something and
	# then renaming it back: nothing to run. A scene saved is reported twice,
	# as a resource and as a scene: both land in `_saves`, and make one run.
	if _ran_with.has(path) and _ran_with[path] == FileAccess.get_md5(path):
		_saves.erase(path)
		_waiting.erase(path)
		_unstale(path)
		return
	_saves[path] = true
	_debounce.start()
	_dock.results_changed.emit()


## Runs the tests of the saved files that compile now. A file that doesn't
## compile keeps its tests waiting: they would only fail to load.
func run_waiting() -> void:
	var files := {}
	var reasons := PackedStringArray()
	for path: String in _waiting.keys():
		var state: String = compile_state.call(path) if compile_state.is_valid() else "clean"
		if state != "clean":
			continue
		for file: String in _waiting[path]:
			files[file] = true
		reasons.append(path.get_file())
		_waiting.erase(path)
	_dock.update_status()
	if files.is_empty():
		return
	var reason := "affected by " + ", ".join(reasons)
	var targets := PackedStringArray(files.keys())
	if files.size() == _dock.test_files().size():
		targets = PackedStringArray([_dock.test_folder])
	if _dock.is_running():
		_dock.restart(targets, reason)
	else:
		_dock.run(targets, reason)


## Remembers the content of `path` as the one the tests ran with, when
## nothing is known of it yet: the script opened, before it is edited.
func remember(path: String) -> void:
	if path != "" and not _ran_with.has(path) and FileAccess.file_exists(path):
		_ran_with[path] = FileAccess.get_md5(path)


## Whether no save is waiting to be looked at, or to compile.
func is_idle() -> bool:
	return _saves.is_empty() and _waiting.is_empty()


## The saved files whose tests wait for them to compile.
func blocked() -> Array:
	return _waiting.keys().filter(
		func(path: String) -> bool:
			return compile_state.is_valid() and compile_state.call(path) == "errors"
	)


## A run of `targets` starts: the saved files whose tests it all runs.
func run_starting(targets: PackedStringArray) -> void:
	_run_covers = {}
	for path: String in _stale_by.keys():
		var covered := (_stale_by[path] as Array).all(
			func(key: String) -> bool:
				return Result.in_targets(key.get_slice("::", 0), key.get_slice("::", 1), targets)
		)
		if covered:
			_run_covers[path] = [FileAccess.get_md5(path), _stale_by[path]]


## The run ended: unless stopped, what the files it covers held when it
## started is what their tests ran with. Unless saved again since.
func run_ended(interrupted: bool) -> void:
	if not interrupted:
		for path: String in _run_covers:
			var cover: Array = _run_covers[path]
			if is_same(_stale_by.get(path), cover[1]):
				_ran_with[path] = cover[0]
				_stale_by.erase(path)
	_run_covers = {}


## No save for SAVE_DELAY: asks gdtest which tests the saved files affect.
func _on_saves_settled() -> void:
	var gdtest := Runner.gdtest_program()
	var saved := _saves.keys()
	_saves.clear()
	for path: String in saved:
		var found := {}
		var options := PackedStringArray(["--list", "--affected-by", path])
		var args: PackedStringArray = _dock.gdtest_args(
			options, PackedStringArray([_dock.test_folder])
		)
		var on_line := func(line: String) -> void: _dock.add_listed(line, found)
		if gdtest == "" or _dock.runner.stream(
			gdtest, args, on_line, _on_affected.bind(path, found)
		) < 0:
			_on_affected(1, path, found)


## gdtest listed the tests `path` affects in `found`: file -> [test name].
func _on_affected(code: int, path: String, found: Dictionary) -> void:
	if code != 0 and found.is_empty():
		# A gdtest older than --affected-by: a test script affects its own
		# tests, any other file every test.
		var tests: Dictionary = _dock.test_files()
		for file: String in tests:
			if file == path or not tests.has(path):
				found[file] = tests[file]
	if found.is_empty():
		return
	_stale_by[path] = _dock.mark_stale(found)
	if not enabled:
		_dock.results_changed.emit()
		return
	_waiting[path] = found
	run_waiting()


## `path` is back to what the tests ran with: the tests its save made stale
## are up to date again, unless another file's save made them stale too.
func _unstale(path: String) -> void:
	var keys: Array = _stale_by.get(path, [])
	_stale_by.erase(path)
	var fresh := []
	for key: String in keys:
		if not _stale_by.values().any(func(k: Array) -> bool: return key in k):
			fresh.append(key)
	_dock.mark_up_to_date(fresh)
