@tool
extends RefCounted
## The project's problems, as each tool last reported them, and how far they
## can be trusted: which tools reported, and which scripts were edited since.

const Problem = preload("problem.gd")

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

## The sources that reported at least once: until all have, the project's
## state is not known.
var _reported := {}
## Scripts edited since they were checked, with problems or not.
var _unchecked := {}


## Replaces what `source` found in `files` ("res://" paths), or in the whole
## project when `files` is empty.
func replace(source: String, files: PackedStringArray, found: Array) -> void:
	var by_file: Dictionary = problems[source]
	if files.is_empty():
		_reported[source] = true
		by_file.clear()
		stale.clear()
		_unchecked.clear()
	else:
		for path in files:
			by_file.erase(path)
			stale.erase(path)
			_unchecked.erase(path)
	for problem: Dictionary in found:
		problem.source = source
		if not by_file.has(problem.file):
			by_file[problem.file] = []
		by_file[problem.file].append(problem)
	if source in [Problem.SOURCE_LINT, Problem.SOURCE_HEALTH]:
		_drop_duplicates()


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
	_unchecked[path] = true
	if has_problems:
		stale[path] = true
	return changed


## Whether every tool a check covers has reported once.
func is_known() -> bool:
	return Problem.SOURCES.all(
		func(source: String) -> bool:
			return _reported.has(source) or not Problem.is_checked(source)
	)


## Whether the project has no problem, as far as the checks of what every
## script holds now can tell: all reported, and no script was edited since.
func is_clean() -> bool:
	var count := counts()
	return is_known() and _unchecked.is_empty() and count.errors + count.warnings == 0


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


## {errors, warnings} over the whole project, of one source or of all.
func counts(source := "") -> Dictionary:
	var count := { "errors": 0, "warnings": 0 }
	for each: String in problems:
		if source != "" and each != source:
			continue
		for path: String in problems[each]:
			for problem: Dictionary in problems[each][path]:
				count["errors" if problem.severity == "error" else "warnings"] += 1
	return count


## path -> {errors, warnings} of the problems `is_shown` lets through.
func counts_by_script(is_shown: Callable) -> Dictionary:
	var by_script := {}
	for path: String in files():
		var count := { "errors": 0, "warnings": 0 }
		for problem: Dictionary in problems_for(path).filter(is_shown):
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
