@tool
extends Node
## Checks the project for the Problems dock: formatting, lint, the compile
## check and code health, as far as the settings turn them on. Fixes what can
## be fixed: on Check, when a script is saved, or when asked.

## A check of the whole project, asked for, started or ended.
signal checking_changed
## The compile check reported.
signal compiled
## The addon wrote these files ("res://" paths) itself, fixing them: not a
## change from outside the editor.
signal files_written(paths: PackedStringArray)

const Runner = preload("runner.gd")
const Settings = preload("settings.gd")
const Problem = preload("problem.gd")
const ProblemsDock = preload("problems_dock.gd")
const ScriptEdits = preload("script_edits.gd")

## Seconds after a save before what it affects is compiled.
const COMPILE_DELAY := 1.5
## Seconds without a save before the whole project is compiled again, after
## compiling only what saves affect: it finds what the links between files
## don't tell, as a class_name removed that other scripts still use.
const FULL_COMPILE_DELAY := 30.0
## The scripts given to one command: Windows limits a command's length.
const FILES_PER_COMMAND := 100
## After a save, the whole project is compiled instead of what the save
## affects when that is more than this share of the project: compiling half
## the project costs about as much as compiling it all, as most of the time
## goes to Godot starting and loading the project, which both do.
const WHOLE_PROJECT_SHARE := 0.5
## The most files compiled by name after a save, for the command's length.
const MOST_COMPILED_BY_NAME := 400
## Seconds without typing before the script being edited is linted.
const LINT_DELAY := 0.3
## What the compile check reads.
const COMPILED_EXTENSIONS := ["gd", "tscn", "tres", "scn", "res"]

var runner: Runner
var dock: ProblemsDock
## Set while this addon saves a script, so its own save starts nothing.
var saving := false
## A check of the whole project is going.
var checking := false

var _compile_timer: Timer
var _full_compile_timer: Timer
var _compile_running := false
## Compile again once the compile check going ends: what was saved since, or
## the whole project when `_compile_whole_again`.
var _compile_again := false
var _compile_whole_again := false
## The files saved since the compile check last started: it compiles them
## and the scripts that use them, as gdtest tells.
var _to_compile := {}
var _health_running := false
var _health_again := false
## When the compile check and code health running started, for what they
## report: a file changed after is still out of date.
var _compile_started := -1
var _health_started := -1
## Lint as you type: the script being edited, waiting for the typing to
## stop, and whether a lint of it is going, or is to start again once it ends.
var _live_timer: Timer
var _live_edit: CodeEdit
var _live_path := ""
var _live_running := false
var _live_again := false
## The sources a check waits for before it fixes, and whether it fixes.
var _waiting_for := {}
var _fixes := false


func _init() -> void:
	_compile_timer = Timer.new()
	_compile_timer.one_shot = true
	_compile_timer.wait_time = COMPILE_DELAY
	_compile_timer.timeout.connect(_on_saves_settled)
	add_child(_compile_timer)
	_full_compile_timer = Timer.new()
	_full_compile_timer.one_shot = true
	_full_compile_timer.wait_time = FULL_COMPILE_DELAY
	_full_compile_timer.timeout.connect(_on_idle)
	add_child(_full_compile_timer)
	_live_timer = Timer.new()
	_live_timer.one_shot = true
	_live_timer.wait_time = LINT_DELAY
	_live_timer.timeout.connect(_lint_live)
	add_child(_live_timer)


# --- Checking ----------------------------------------------------------------


## Checks the whole project. `fix`: first fixes what can be fixed, in the
## scripts without unsaved changes.
func check(fix: bool) -> void:
	if checking or runner.program == "":
		return
	checking = true
	_fixes = fix
	checking_changed.emit()
	_waiting_for = {}
	for source: String in [Problem.SOURCE_FORMAT, Problem.SOURCE_LINT]:
		if Problem.is_checked(source):
			_waiting_for[source] = true
	if _waiting_for.is_empty():
		_after_fixing()
		return
	var sources := _waiting_for.keys()
	for source: String in sources:
		check_source(source)


## Runs one tool again on the whole project.
func check_source(source: String) -> void:
	match source:
		Problem.SOURCE_FORMAT:
			format_check(PackedStringArray())
		Problem.SOURCE_LINT:
			lint(PackedStringArray())
		Problem.SOURCE_TYPES:
			compile()
		Problem.SOURCE_HEALTH:
			health()


## A script was saved: fixes and checks it, as the settings say.
func script_saved(script: GDScript) -> void:
	var path := script.resource_path
	if not Settings.value(Settings.CHECK_ON_SAVE):
		return
	if Settings.value(Settings.AUTOFIX):
		_fix_saved(script)
	_check_files(PackedStringArray([path]))
	_saved(path)


## A scene or resource was saved: the compile check and code health read
## them too, and run again after it, as after a script's save.
func resource_saved(path: String) -> void:
	if Settings.value(Settings.CHECK_ON_SAVE):
		_saved(path)


func _saved(path: String) -> void:
	_to_compile[path] = true
	_compile_timer.start()
	_full_compile_timer.stop()


## The script at `path` was edited in `code_edit`: once the typing stops,
## lints the text as it is, saved or not.
func text_edited(path: String, code_edit: CodeEdit) -> void:
	if not Settings.value(Settings.LINT_AS_YOU_TYPE) or path.get_extension() != "gd":
		return
	if not Problem.is_checked(Problem.SOURCE_LINT):
		return
	_live_path = path
	_live_edit = code_edit
	_live_timer.start()


func _lint_live() -> void:
	if runner.program == "" or not is_instance_valid(_live_edit):
		return
	if _live_running:
		_live_again = true
		return
	var path := _live_path
	var input := OS.get_temp_dir().path_join("gdsuite-lint-%d.gd" % OS.get_process_id())
	var file := FileAccess.open(input, FileAccess.WRITE)
	if file == null:
		return
	file.store_string(_live_edit.text)
	file.close()
	_live_running = true
	var args := PackedStringArray(["lint", "--report", "json", "--input", input, "--as", path])
	runner.run(args, _on_live_linted.bind(path))


func _on_live_linted(result: Dictionary, path: String) -> void:
	_live_running = false
	var report: Variant = result.json
	# Text that doesn't parse, as often while typing, keeps what was found:
	# its syntax error is Godot's to show. A gdsuite older than
	# `lint --input` answers no report.
	if report is Dictionary and report.has("findings") and report.get("skipped", []).is_empty():
		dock.set_live_problems(path, Problem.from_report(report))
	if _live_again:
		_live_again = false
		_lint_live()


## Whether the script at `path` compiles, for the tests to run on save:
## "pending" while the compile check after a save has yet to report. Without
## that check, they don't wait.
func compile_state(path: String) -> String:
	var compiles: bool = (
		Settings.value(Settings.CHECK_ON_SAVE) and Problem.is_checked(Problem.SOURCE_TYPES)
	)
	if not compiles or path.get_extension() != "gd":
		return "clean"
	if _compile_running or not _compile_timer.is_stopped():
		return "pending"
	var has_error := dock.store.problems_for(path).any(
		func(p: Dictionary) -> bool:
			return p.source == Problem.SOURCE_TYPES and p.severity == "error"
	)
	return "errors" if has_error else "clean"


## Format and lint reported: fixes what they found, and only then compiles
## and measures, so that the lines these report are the ones after the fixes.
func _source_reported(source: String) -> void:
	if not _waiting_for.has(source):
		return
	_waiting_for.erase(source)
	if not _waiting_for.is_empty():
		return
	var fixable := dock.store.fixable_files()
	if not _fixes or fixable.lint.size() + fixable.format.size() == 0:
		_after_fixing()
		return
	fix(
		fixable.lint, fixable.format,
		func(fixed: int) -> void:
			_after_fixing()
			if fixed > 0:
				ScriptEdits.toast("Fixed %d script%s" % [fixed, "" if fixed == 1 else "s"])
	)


func _after_fixing() -> void:
	_compile_and_measure()
	_end_check()


func _compile_and_measure() -> void:
	if Problem.is_checked(Problem.SOURCE_TYPES):
		compile()
	if Problem.is_checked(Problem.SOURCE_HEALTH):
		health()


## No save for COMPILE_DELAY: compiles what the saves affect, and measures the
## project again.
func _on_saves_settled() -> void:
	if Problem.is_checked(Problem.SOURCE_TYPES):
		_compile_saved()
	if Problem.is_checked(Problem.SOURCE_HEALTH):
		health()


## No save for FULL_COMPILE_DELAY since the compile check of what saves
## affect: compiles the whole project, for what that one could not see.
func _on_idle() -> void:
	if Settings.value(Settings.CHECK_ON_SAVE) and Problem.is_checked(Problem.SOURCE_TYPES):
		compile()


## A check ends once the compile check and code health reported too.
func _end_check() -> void:
	if checking and _waiting_for.is_empty() and not _compile_running and not _health_running:
		checking = false
		checking_changed.emit()


func _check_files(paths: PackedStringArray) -> void:
	if Problem.is_checked(Problem.SOURCE_FORMAT):
		format_check(paths)
	if Problem.is_checked(Problem.SOURCE_LINT):
		lint(paths)


## Finds the scripts of `paths` ("res://" paths), or of the project, that
## formatting would change.
func format_check(paths: PackedStringArray) -> void:
	if runner.program == "":
		return
	var args := PackedStringArray(["fmt", "--", "--check", "--format", "json"])
	dock.set_running(Problem.SOURCE_FORMAT, true)
	runner.run(args + _targets(paths), _on_format_check_done.bind(paths))


func _on_format_check_done(result: Dictionary, paths: PackedStringArray) -> void:
	dock.set_running(Problem.SOURCE_FORMAT, false)
	if result.json is Dictionary:
		dock.set_problems(Problem.SOURCE_FORMAT, paths, Problem.from_format_report(result.json))
	else:
		dock.set_status("The format check could not run: " + str(result.output).strip_edges())
	if paths.is_empty():
		_source_reported(Problem.SOURCE_FORMAT)


## Lints `paths` ("res://" paths), or the whole project when it is empty.
func lint(paths: PackedStringArray) -> void:
	if runner.program == "":
		return
	var args := PackedStringArray(["lint", "--report", "json", "--"])
	dock.set_running(Problem.SOURCE_LINT, true)
	runner.run(args + _targets(paths), _on_lint_done.bind(paths))


func _on_lint_done(result: Dictionary, paths: PackedStringArray) -> void:
	dock.set_running(Problem.SOURCE_LINT, false)
	if result.json is Dictionary:
		dock.set_problems(Problem.SOURCE_LINT, paths, Problem.from_report(result.json))
	else:
		dock.set_status("Lint could not run: " + str(result.output).strip_edges())
	if paths.is_empty():
		_source_reported(Problem.SOURCE_LINT)


## Compiles the whole project.
func compile() -> void:
	if runner.program == "":
		return
	if _compile_running:
		_compile_again = true
		_compile_whole_again = true
		return
	_begin_compile()
	_run_compile(PackedStringArray())


## Compiles the files saved since the compile check last started, and the
## files that use them, as `gdsuite dependents` tells: those a save can
## break. The whole project when the project's settings changed, when that is
## most of the project, or when gdsuite can't tell.
func _compile_saved() -> void:
	if runner.program == "":
		return
	if _compile_running:
		_compile_again = true
		return
	var saved := PackedStringArray(_to_compile.keys())
	_begin_compile()
	if saved.is_empty() or "res://project.godot" in saved:
		_run_compile(PackedStringArray())
		return
	var lines := PackedStringArray()
	var args := PackedStringArray(["--project-root", Runner.project_root(), "dependents"])
	args.append_array(saved)
	var on_line := func(line: String) -> void: lines.append(line)
	if runner.stream(runner.program, args, on_line, _on_dependents.bind(lines)) < 0:
		_run_compile(PackedStringArray())


## gdsuite printed the files the saves affect: compiles them.
func _on_dependents(code: int, lines: PackedStringArray) -> void:
	var json: Variant = JSON.parse_string("".join(lines)) if code == 0 else null
	if not json is Dictionary or not json.get("files") is Array:
		_run_compile(PackedStringArray())
		return
	var targets := PackedStringArray()
	for path: String in json.files:
		# The compile check leaves addons out of the project, as Godot's own
		# warnings do; a deleted file can't be compiled.
		if path.begins_with("res://addons/") or not FileAccess.file_exists(path):
			continue
		if path.get_extension() in COMPILED_EXTENSIONS:
			targets.append(path)
	var project_files: float = json.get("project_files", 0)
	if (
		targets.size() > MOST_COMPILED_BY_NAME
		or targets.size() > project_files * WHOLE_PROJECT_SHARE
	):
		targets = PackedStringArray()
	elif targets.is_empty():
		# Nothing it could break: an addon's file, or one deleted.
		_finish_compile()
		return
	_run_compile(targets)


func _begin_compile() -> void:
	_compile_running = true
	_compile_started = Time.get_ticks_usec()
	_to_compile.clear()
	dock.set_running(Problem.SOURCE_TYPES, true)


## Compiles `targets` ("res://" paths), or the whole project when empty.
func _run_compile(targets: PackedStringArray) -> void:
	var args := PackedStringArray(["typecheck", "--report", "json"])
	if not targets.is_empty():
		args.append("--")
		args.append_array(_targets(targets))
	runner.run(args, _on_compile_done.bind(targets))


func _on_compile_done(result: Dictionary, targets: PackedStringArray) -> void:
	if result.json is Dictionary:
		var problems := Problem.from_report(result.json)
		# Godot reports lines only: a column would be made up.
		for problem: Dictionary in problems:
			problem.column = 0
		# Compiling a script compiles what it uses too, whose problems it may
		# report: only those of the targets replace what was known of them.
		if not targets.is_empty():
			problems = problems.filter(func(p: Dictionary) -> bool: return p.file in targets)
		dock.set_problems(Problem.SOURCE_TYPES, targets, problems, _compile_started)
		if not targets.is_empty() and Settings.value(Settings.CHECK_ON_SAVE):
			_full_compile_timer.start()
		elif targets.is_empty():
			_full_compile_timer.stop()
	else:
		dock.set_status("The compile check could not run: " + str(result.output).strip_edges())
	_finish_compile()


func _finish_compile() -> void:
	_compile_running = false
	dock.set_running(Problem.SOURCE_TYPES, false)
	if _compile_again:
		_compile_again = false
		if _compile_whole_again:
			_compile_whole_again = false
			compile()
		else:
			_compile_saved()
	compiled.emit()
	_end_check()


func health() -> void:
	if runner.program == "":
		return
	if _health_running:
		_health_again = true
		return
	_health_running = true
	_health_started = Time.get_ticks_usec()
	dock.set_running(Problem.SOURCE_HEALTH, true)
	runner.run(PackedStringArray(["health", "--quick", "--report", "json"]), _on_health_done)


func _on_health_done(result: Dictionary) -> void:
	_health_running = false
	dock.set_running(Problem.SOURCE_HEALTH, false)
	if result.json is Dictionary:
		dock.set_graph(result.json.get("graph", {}))
		var problems := Problem.from_health_report(result.json)
		dock.set_problems(Problem.SOURCE_HEALTH, PackedStringArray(), problems, _health_started)
	else:
		dock.set_status("Code health could not run: " + str(result.output).strip_edges())
	if _health_again:
		_health_again = false
		health()
	_end_check()


# --- Fixing ------------------------------------------------------------------


## Applies the safe lint fixes to `to_lint`, and formats `to_format`, on disk,
## then shows the result in their open editors and checks them again. Scripts
## with unsaved changes are left alone: they would be lost. Calls `done` with
## the number of scripts fixed.
func fix(to_lint: Array, to_format: Array, done := Callable()) -> void:
	var skipped := {}
	var kept := func(path: String) -> bool:
		if ScriptEdits.is_unsaved(path):
			skipped[path] = true
		return not skipped.has(path)
	var lint_files := PackedStringArray(to_lint.filter(kept))
	var format_files := PackedStringArray(to_format.filter(kept))
	if not skipped.is_empty():
		ScriptEdits.toast("%d script%s with unsaved changes left as they are: save first" % [
			skipped.size(), "" if skipped.size() == 1 else "s",
		], true)
	var files := {}
	for path in lint_files + format_files:
		files[path] = FileAccess.get_md5(path)
	if files.is_empty():
		if done.is_valid():
			done.call(0)
		return
	# A lint fix can leave code to format: a formatted script stays so.
	if Problem.is_checked(Problem.SOURCE_FORMAT):
		for path in lint_files:
			if not path in format_files and not dock.store.is_unformatted(path):
				format_files.append(path)
	var commands := []
	for group in _groups(lint_files):
		commands.append(PackedStringArray(["lint", "--", "--fix"]) + _targets(group))
	for group in _groups(format_files):
		commands.append(PackedStringArray(["fmt", "--"]) + _targets(group))
	_run_all(commands, func() -> void: _on_fixed(files, done))


func _on_fixed(files: Dictionary, done: Callable) -> void:
	var changed := PackedStringArray()
	for path: String in files:
		if FileAccess.get_md5(path) != files[path]:
			changed.append(path)
			ScriptEdits.reload_file(path)
	files_written.emit(changed)
	if not changed.is_empty():
		for group in _groups(changed):
			_check_files(group)
	if done.is_valid():
		done.call(changed.size())


## Runs `commands` one after the other, then calls `done`.
func _run_all(commands: Array, done: Callable) -> void:
	if commands.is_empty():
		done.call()
		return
	var next := func(_result: Dictionary) -> void: _run_all(commands.slice(1), done)
	runner.run(commands[0], next)


## Fixes a script that was just saved, saves it again, and shows it in its
## open editor.
func _fix_saved(script: GDScript) -> void:
	var path := script.resource_path
	var text := script.source_code
	if Problem.is_checked(Problem.SOURCE_LINT):
		var file := PackedStringArray([path])
		runner.run_now(PackedStringArray(["lint", "--", "--fix"]) + _targets(file))
		text = FileAccess.get_file_as_string(path)
	if Problem.is_checked(Problem.SOURCE_FORMAT):
		var formatted: Variant = format_text(text)
		if formatted != null:
			text = formatted
	if text == script.source_code or text == "":
		return
	script.source_code = text
	saving = true
	ResourceSaver.save(script)
	files_written.emit(PackedStringArray([path]))
	var code_edit := ScriptEdits.code_edit_for(path)
	if code_edit != null:
		ScriptEdits.replace_text(code_edit, text, true)
	saving = false


## Formats the script being edited, saved or not.
func format_current_script() -> void:
	var script := EditorInterface.get_script_editor().get_current_script()
	var code_edit := ScriptEdits.current_code_edit()
	if not script is GDScript or code_edit == null:
		ScriptEdits.toast("Open a GDScript file to format it")
		return
	var formatted: Variant = format_text(code_edit.text)
	if formatted != null and formatted != code_edit.text:
		ScriptEdits.replace_text(code_edit, formatted, false)


## `text` formatted with the project's settings, or null when gdsuite leaves
## it as it is.
func format_text(text: String) -> Variant:
	if runner.program == "":
		ScriptEdits.toast("gdsuite not found. Install GDSuite: " + Problem.INSTALL_DOCS, true)
		return null
	var input := OS.get_temp_dir().path_join("gdsuite-format-%d.gd" % OS.get_process_id())
	var file := FileAccess.open(input, FileAccess.WRITE)
	if file == null:
		return null
	file.store_string(text)
	file.close()
	var result := runner.run_now(PackedStringArray(["fmt", "--input", input]))
	DirAccess.remove_absolute(input)
	if result.code != 0:
		ScriptEdits.toast(
			"The script was left as it is: it has a syntax error, "
			+ "or formatting would change what it does",
			true,
		)
		return null
	# Never replace a script with nothing.
	if str(result.output).strip_edges() == "" and text.strip_edges() != "":
		ScriptEdits.toast("The script was left as it is: gdsuite returned no text", true)
		return null
	return result.output


# --- Helpers -----------------------------------------------------------------


## `paths` as gdsuite's arguments: the whole project when there is none.
static func _targets(paths: PackedStringArray) -> PackedStringArray:
	if paths.is_empty():
		return PackedStringArray(["."])
	var targets := PackedStringArray()
	for path in paths:
		targets.append(path.trim_prefix("res://"))
	return targets


static func _groups(paths: PackedStringArray) -> Array[PackedStringArray]:
	var groups: Array[PackedStringArray] = []
	for start in range(0, paths.size(), FILES_PER_COMMAND):
		groups.append(paths.slice(start, start + FILES_PER_COMMAND))
	return groups
