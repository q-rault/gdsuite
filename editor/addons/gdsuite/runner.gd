@tool
extends RefCounted
## Runs the gdsuite program for the addon. Long commands run on a thread, so
## the editor never waits for them.

const Settings = preload("settings.gd")

var program := ""
var _threads: Array[Thread] = []
## Processes `stream` started and that are still running.
var _pids: Array[int] = []


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


## The gdtest gdsuite keeps in the project for the editor: gdsuite itself,
## started as gdtest. Empty when the project has none yet.
static func gdtest_program() -> String:
	var path := project_root().path_join(".gdsuite/tools/gdtest/bin/gdtest")
	if OS.get_name() == "Windows":
		path += ".exe"
	return path if FileAccess.file_exists(path) else ""


## Starts `path` with `args`, calls `on_line` with each line it prints, then
## `on_exit` with its exit code, all on the main thread. Returns the process
## id, or -1 when it could not start.
func stream(path: String, args: PackedStringArray, on_line: Callable, on_exit: Callable) -> int:
	# Not blocking: on Windows, a pipe can stay open after the process ends,
	# so reading until its end would never return.
	var process := OS.execute_with_pipe(path, args, false)
	if process.is_empty():
		return -1
	var pid: int = process.pid
	_pids.append(pid)
	var reader := Thread.new()
	_threads.append(reader)
	reader.start(_read_lines.bind(reader, process, on_line, on_exit))
	return pid


## Ends a process `stream` started.
func kill(pid: int) -> void:
	if pid > 0 and OS.is_process_running(pid):
		OS.kill(pid)


## Reads the process's output until it ended and nothing is left. What it
## prints on stderr is read too, and dropped: a full pipe would stop it.
func _read_lines(thread: Thread, process: Dictionary, on_line: Callable, on_exit: Callable) -> void:
	var pid: int = process.pid
	var stdout: FileAccess = process.stdio
	var stderr: FileAccess = process.stderr
	var pending := PackedByteArray()
	while true:
		# Checked before reading: once it ended, one more read gets the rest.
		var running := OS.is_process_running(pid)
		var chunk := stdout.get_buffer(65536)
		stderr.get_buffer(65536)
		if chunk.is_empty():
			if not running:
				break
			OS.delay_msec(10)
			continue
		pending.append_array(chunk)
		var newline := pending.find(10)
		while newline >= 0:
			var line := pending.slice(0, newline).get_string_from_utf8().strip_edges()
			pending = pending.slice(newline + 1)
			if line != "":
				_deliver.call_deferred(on_line, line)
			newline = pending.find(10)
	var last := pending.get_string_from_utf8().strip_edges()
	if last != "":
		_deliver.call_deferred(on_line, last)
	stdout.close()
	stderr.close()
	var code := OS.get_process_exit_code(pid)
	_stream_done.call_deferred(thread, pid, on_exit, code)


func _deliver(on_line: Callable, line: String) -> void:
	if on_line.is_valid():
		on_line.call(line)


func _stream_done(thread: Thread, pid: int, on_exit: Callable, code: int) -> void:
	thread.wait_to_finish()
	_threads.erase(thread)
	_pids.erase(pid)
	if on_exit.is_valid():
		on_exit.call(code)


## Ends the processes still running and waits for their threads, before the
## addon is unloaded.
func stop() -> void:
	for pid in _pids:
		kill(pid)
	_pids.clear()
	for thread in _threads:
		if thread.is_started():
			thread.wait_to_finish()
	_threads.clear()
