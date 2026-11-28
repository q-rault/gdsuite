@tool
extends RefCounted
## The project's problems, as each tool last reported them, and how far they
## can be trusted: which tools reported, and which scripts were edited since.

const Problem = preload("problem.gd")
const Status = preload("status.gd")

## source -> { "res://path.gd": [problem, ...] }. A problem is a Dictionary:
## {file, line, column, severity, message, rule, fixable, source}.
var problems := {
	Problem.SOURCE_FORMAT: {},
	Problem.SOURCE_LINT: {},
	Problem.SOURCE_TYPES: {},
	Problem.SOURCE_HEALTH: {},
}
## Scripts with problems edited since they were checked: they may be gone.
var stale := {}
## Scripts whose lint problems are those of the text being edited, linted as
## it is typed: up to date, while the others of the script may be gone.
var live := {}

## The sources that reported at least once: until all have, the project's
## state is not known.
var _reported := {}
## Scripts edited since they were checked, with problems or not: path ->
## when last edited, in Time.get_ticks_usec().
var _unchecked := {}
## The sources that check the whole project at once (the compile check, code
## health), with the files changed on disk since they last did: source ->
## {"res://path": when, in Time.get_ticks_usec()}. A change to one script
## can change what they find in another, so all they found is out of date,
## not only that script's.
var _changed_since := {
	Problem.SOURCE_TYPES: {},
	Problem.SOURCE_HEALTH: {},
}


## Replaces what `source` found in `files` ("res://" paths), or in the whole
## project when `files` is empty. `started`, in Time.get_ticks_usec(): when
## that check started; a file changed after it is still out of date.
func replace(source: String, files: PackedStringArray, found: Array, started := -1) -> void:
	var by_file: Dictionary = problems[source]
	if files.is_empty():
		_reported[source] = true
		by_file.clear()
		stale.clear()
		if source == Problem.SOURCE_LINT:
			live.clear()
		# An edit made while the check ran is still to check.
		for path: String in _unchecked.keys():
			if started < 0 or _unchecked[path] < started:
				_unchecked.erase(path)
		if _changed_since.has(source):
			_forget_changes(source, _changed_since[source].keys(), started)
	else:
		for path in files:
			by_file.erase(path)
			stale.erase(path)
			if source == Problem.SOURCE_LINT:
				live.erase(path)
			_unchecked.erase(path)
		if _changed_since.has(source):
			_forget_changes(source, files, started)
	for problem: Dictionary in found:
		problem.source = source
		if not by_file.has(problem.file):
			by_file[problem.file] = []
		by_file[problem.file].append(problem)
	if source in [Problem.SOURCE_LINT, Problem.SOURCE_HEALTH]:
		_drop_duplicates()


## Replaces what lint found in `path` with what it found in the text being
## edited. The script stays edited since checked: the other tools haven't
## seen that text.
func replace_live(path: String, found: Array) -> void:
	var by_file: Dictionary = problems[Problem.SOURCE_LINT]
	by_file.erase(path)
	for problem: Dictionary in found:
		problem.source = Problem.SOURCE_LINT
		if not by_file.has(problem.file):
			by_file[problem.file] = []
		by_file[problem.file].append(problem)
	live[path] = true
	_drop_duplicates()


## Whether `problem` may be gone: its script was edited since it was found.
func is_faded(problem: Dictionary) -> bool:
	if not stale.has(problem.file):
		return false
	return not (problem.source == Problem.SOURCE_LINT and live.has(problem.file))


## Drops the code health findings lint made too, with a rule of its own.
func _drop_duplicates() -> void:
	var health: Dictionary = problems[Problem.SOURCE_HEALTH]
	var lint: Dictionary = problems[Problem.SOURCE_LINT]
	for path: String in health.keys():
		var made_by_lint := {}
		for problem: Dictionary in lint.get(path, []):
			made_by_lint["%s:%d" % [problem.rule, problem.line]] = true
			made_by_lint["%s:0" % problem.rule] = true
		health[path] = health[path].filter(
			func(p: Dictionary) -> bool:
				var rule: String = Problem.SAME_AS_LINT.get(p.rule, "")
				return rule == "" or not made_by_lint.has("%s:%d" % [rule, p.line])
		)
		if health[path].is_empty():
			health.erase(path)


## Forgets what the tools a check doesn't cover found.
func forget_unchecked_sources() -> void:
	for source: String in Problem.SOURCES:
		if not Problem.is_checked(source):
			problems[source].clear()
			_reported.erase(source)


## The script at `path` was edited since it was checked. Returns whether that
## changed anything: whether the project is clean, or how its problems show.
func mark_stale(path: String) -> bool:
	var has_problems := problems.values().any(
		func(by_file: Dictionary) -> bool: return by_file.has(path)
	)
	var changed := not _unchecked.has(path) or (has_problems and not stale.has(path))
	_unchecked[path] = Time.get_ticks_usec()
	if has_problems:
		stale[path] = true
	return changed


## Forgets the changes to `paths` that `source` saw: those before `started`,
## or all of them when it is -1.
func _forget_changes(source: String, paths: Array, started: int) -> void:
	var changed: Dictionary = _changed_since[source]
	for path: String in paths:
		if changed.has(path) and (started < 0 or changed[path] < started):
			changed.erase(path)


## A file of the project changed on disk, saved or changed outside the
## editor: what the compile check and code health found is out of date.
## Returns whether that changed anything.
func changed_on_disk(path: String) -> bool:
	var changed := false
	for source: String in _changed_since:
		if Problem.is_checked(source):
			changed = changed or not _changed_since[source].has(path)
			_changed_since[source][path] = Time.get_ticks_usec()
	return changed


## The files changed since the checks that cover them last reported: edited
## scripts, and files changed since the compile check or code health ran.
func changed_files() -> Array:
	var found := _unchecked.duplicate()
	for source: String in _changed_since:
		if Problem.is_checked(source):
			found.merge(_changed_since[source])
	return found.keys()


## What the checks found: nothing known until every tool reported once, then
## errors, warnings, or none.
func result() -> String:
	if not is_known():
		return Status.RESULT_NONE
	var count := counts()
	if count.errors > 0:
		return Status.RESULT_ERROR
	if count.warnings > 0:
		return Status.RESULT_WARNING
	return Status.RESULT_OK


## Status.STALE when a file changed since the checks that cover it reported,
## else FRESH.
func freshness() -> String:
	return Status.STALE if not changed_files().is_empty() else Status.FRESH


## Whether every tool a check covers has reported once.
func is_known() -> bool:
	return Problem.SOURCES.all(
		func(source: String) -> bool:
			return _reported.has(source) or not Problem.is_checked(source)
	)


## The problems of one script, every source, sorted by line.
func problems_for(path: String) -> Array:
	var found := []
	for source: String in problems:
		found.append_array(problems[source].get(path, []))
	found.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return a.line < b.line)
	return found


## Every script with a problem.
func files() -> Array:
	var found := {}
	for source: String in problems:
		found.merge(problems[source])
	return found.keys()


func has_error(path: String) -> bool:
	return problems_for(path).any(func(p: Dictionary) -> bool: return p.severity == "error")


## {errors, warnings} over the whole project, of one source or of all. An
## info counts as neither.
func counts(source := "") -> Dictionary:
	var count := { "errors": 0, "warnings": 0 }
	for each: String in problems:
		if source != "" and each != source:
			continue
		for path: String in problems[each]:
			for problem: Dictionary in problems[each][path]:
				if problem.severity != "info":
					count["errors" if problem.severity == "error" else "warnings"] += 1
	return count


## path -> {errors, warnings} of the problems `is_shown` lets through.
func counts_by_script(is_shown: Callable) -> Dictionary:
	var by_script := {}
	for path: String in files():
		var count := { "errors": 0, "warnings": 0 }
		for problem: Dictionary in problems_for(path).filter(is_shown):
			if problem.severity != "info":
				count["errors" if problem.severity == "error" else "warnings"] += 1
		by_script[path] = count
	return by_script


## The scripts Check can fix: {lint: [path, ...], format: [path, ...]}.
func fixable_files() -> Dictionary:
	var lint: Dictionary = problems[Problem.SOURCE_LINT]
	var fixable := func(p: Dictionary) -> bool: return p.fixable
	return {
		"lint": lint.keys().filter(func(path: String) -> bool: return lint[path].any(fixable)),
		"format": problems[Problem.SOURCE_FORMAT].keys(),
	}


func is_unformatted(path: String) -> bool:
	return problems[Problem.SOURCE_FORMAT].has(path)
