@tool
extends Node
## On save, for the Tests dock: a saved file makes the tests it can affect out
## of date, and with On save on, runs them again. gdtest says which tests: those
## that use the file, directly or through other scripts, scenes and resources.
##
## It doesn't run tests for nothing: it waits for the saves to stop, holds
## the tests of a script that doesn't compile, runs nothing for a script saved
## back to what its tests ran with, and a save during a run restarts it rather
## than queue another. A file changed outside the editor (a branch checked
## out, a pull) makes its tests out of date too, but runs nothing: that can be
## hundreds of files, and the run is for the user to ask.

const Runner = preload("runner.gd")
const Result = preload("test_result.gd")

## Seconds without a save before it asks gdtest what the saves affect.
const SAVE_DELAY := 0.4
## Past this many files changed at once, every test is out of date: asking
## gdtest would make too long a command, and most tests would be anyway.
const MAX_ASKED := 50

## Whether saving a file runs the tests it affects again.
var enabled := false
## Says whether a saved script compiles: "clean", "errors", or "pending"
## while the compile check has yet to report. Set by the plugin; a run waits
## for "clean".
var compile_state: Callable

## The Tests dock.
var _dock: Node
## The files saved or changed in the last moments, with whether their tests
## may run again (saved) or are only out of date (changed outside the
## editor): it waits for the saves to stop (`_debounce`), so that several
## saves in a row make one run.
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
	_file_changed(path, true)


## A file of the project changed outside the editor: once the changes stop,
## marks the tests it affects out of date.
func file_changed_outside(path: String) -> void:
	_file_changed(path, false)


func _file_changed(path: String, may_run: bool) -> void:
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
	_saves[path] = _saves.get(path, false) or may_run
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


## No save for SAVE_DELAY: asks gdtest which tests the saved files affect,
## all at once.
func _on_saves_settled() -> void:
	var gdtest := Runner.gdtest_program()
	var saved := _saves.duplicate()
	_saves.clear()
	var found := {}
	if saved.size() > MAX_ASKED:
		_on_affected(1, saved, found)
		return
	var options := PackedStringArray(["--list"])
	for path: String in saved:
		options.append_array(["--affected-by", path])
	var args: PackedStringArray = _dock.gdtest_args(
		options, PackedStringArray([_dock.test_folder])
	)
	var on_line := func(line: String) -> void: _dock.add_listed(line, found)
	if gdtest == "" or _dock.runner.stream(
		gdtest, args, on_line, _on_affected.bind(saved, found)
	) < 0:
		_on_affected(1, saved, found)


## gdtest listed the tests the `saved` files affect in `found`: file -> [test
## name]. `saved`: path -> whether its tests may run again.
func _on_affected(code: int, saved: Dictionary, found: Dictionary) -> void:
	if code != 0 and found.is_empty():
		# Too many files, or a gdtest older than --affected-by: a test script
		# affects its own tests, any other file every test.
		var tests: Dictionary = _dock.test_files()
		var any_other := saved.size() > MAX_ASKED or saved.keys().any(
			func(path: String) -> bool: return not tests.has(path)
		)
		for file: String in tests:
			if any_other or saved.has(file):
				found[file] = tests[file]
	if found.is_empty():
		# No test to run again: the tests are up to date, as before the save.
		_dock.results_changed.emit()
		return
	var stale: Array = _dock.mark_stale(found)
	for path: String in saved:
		_stale_by[path] = stale
	var to_run := saved.keys().filter(func(path: String) -> bool: return saved[path])
	if not enabled or to_run.is_empty():
		_dock.results_changed.emit()
		return
	for path: String in to_run:
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
