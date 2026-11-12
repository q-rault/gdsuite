@tool
extends RefCounted
## Runs the gdsuite program for the addon. Long commands run on a thread, so
## the editor never waits for them.

const Settings = preload("settings.gd")

var program := ""
var _threads: Array[Thread] = []


## The project's folder, where gdsuite runs.
static func project_root() -> String:
	return ProjectSettings.globalize_path("res://")


## Finds gdsuite: the editor setting, then the link gdsuite keeps in the
## project's .gdsuite/bin, then the PATH. Empty when none answers.
func find_program() -> String:
	var candidates: PackedStringArray = []
	var configured := str(Settings.get_personal(Settings.PROGRAM))
	if configured != "":
		candidates.append(configured)
	var link := project_root().path_join(".gdsuite/bin/gdsuite")
	if OS.get_name() == "Windows":
		link += ".exe"
	if FileAccess.file_exists(link):
		candidates.append(link)
	candidates.append("gdsuite")
	for candidate in candidates:
		var output := []
		if OS.execute(candidate, PackedStringArray(["--version"]), output) == 0:
			program = candidate
			return program
	program = ""
	return program


## Runs gdsuite with `args` and waits: for quick commands, such as formatting
## one script. Returns {code, output, json}; `json` is null when the output
## isn't JSON.
func run_now(args: PackedStringArray) -> Dictionary:
	if program == "":
		return { "code": -1, "output": "gdsuite not found", "json": null }
	var full := PackedStringArray(["--project-root", project_root()])
	full.append_array(args)
	var output := []
	var code := OS.execute(program, full, output)
	var text := "" if output.is_empty() else str(output[0])
	var json: Variant = JSON.parse_string(text) if text.strip_edges().begins_with("{") else null
	return { "code": code, "output": text, "json": json }


## Runs gdsuite with `args` on a thread, then calls `done` with the result of
## `run_now` on the main thread.
func run(args: PackedStringArray, done: Callable) -> void:
	var thread := Thread.new()
	_threads.append(thread)
	thread.start(_run_on_thread.bind(thread, args, done))


func _run_on_thread(thread: Thread, args: PackedStringArray, done: Callable) -> void:
	var result := run_now(args)
	_finish.call_deferred(thread, result, done)


func _finish(thread: Thread, result: Dictionary, done: Callable) -> void:
	thread.wait_to_finish()
	_threads.erase(thread)
	if done.is_valid():
		done.call(result)


## Waits for the commands still running, before the addon is unloaded.
func stop() -> void:
	for thread in _threads:
		if thread.is_started():
			thread.wait_to_finish()
	_threads.clear()
