@tool
## A test's result, as the Tests dock shows it: its status, the icon and
## words for it, how long it took, and why it failed. Nothing here holds
## state.

const Ui = preload("ui.gd")

const STATUS_IDLE := "idle"
const STATUS_QUEUED := "queued"
const STATUS_RUNNING := "running"
const STATUS_PASS := "pass"
const STATUS_FAIL := "fail"
const STATUS_ERROR := "error"
const STATUS_SKIP := "skip"


## How the dock and gdtest name a test: "res://test/a.gd::test_b".
static func key(file: String, test_name: String) -> String:
	return "%s::%s" % [file, test_name]


## One of gdtest's JSON events, or {} for a line that isn't one.
static func event(line: String) -> Dictionary:
	var trimmed := line.strip_edges()
	if not trimmed.begins_with("{"):
		return {}
	# JSON.new().parse, unlike JSON.parse_string, prints nothing on bad input.
	var json := JSON.new()
	if json.parse(trimmed) != OK:
		return {}
	return json.data if json.data is Dictionary else {}


## The worse of two statuses: what a script shows for its tests.
static func worse(a: String, b: String) -> String:
	var order := [STATUS_IDLE, STATUS_SKIP, STATUS_PASS, STATUS_QUEUED, STATUS_RUNNING]
	order.append_array([STATUS_FAIL, STATUS_ERROR])
	return a if order.find(a) >= order.find(b) else b


static func describe(status: String) -> String:
	var descriptions := {
		STATUS_PASS: "Passed",
		STATUS_FAIL: "Failed",
		STATUS_ERROR: "Could not run",
		STATUS_SKIP: "Skipped",
		STATUS_QUEUED: "Waiting to run",
		STATUS_RUNNING: "Running",
	}
	return descriptions.get(status, "Not run yet")


## The icon of `status`; a running test's spinner shows `frame`.
static func icon(status: String, frame: int) -> Texture2D:
	var theme := EditorInterface.get_editor_theme()
	match status:
		# Coloured marks without a badge behind them, as in Vitest.
		STATUS_PASS:
			return theme.get_icon("ImportCheck", "EditorIcons")
		STATUS_FAIL, STATUS_ERROR:
			return theme.get_icon("ImportFail", "EditorIcons")
		STATUS_SKIP:
			return theme.get_icon("NodeWarning", "EditorIcons")
		STATUS_QUEUED:
			return theme.get_icon("Time", "EditorIcons")
		STATUS_RUNNING:
			return theme.get_icon("Progress%d" % (frame + 1), "EditorIcons")
	return null


## "12 ms", "1.24 s", "2 m 05 s", nothing under 1 ms: short, and of one width
## at a glance, to compare.
static func duration(us: int) -> String:
	# Under a millisecond, the time says nothing worth reading.
	if us < 1000:
		return ""
	if us < 1000000:
		return "%d ms" % roundi(us / 1000.0)
	if us < 60000000:
		return "%.2f s" % (us / 1000000.0)
	var seconds := roundi(us / 1000000.0)
	return "%d m %02d s" % [seconds / 60, seconds % 60]


## Why a test failed, as BBCode lines, as Vitest shows it: its name, which
## opens it, each failure, then where: the line, which opens the script there,
## its code, and the test's local variables then.
static func failure_lines(file: String, test_name: String, result: Dictionary) -> PackedStringArray:
	var dim := Ui.faded(Ui.text_color()).to_html()
	var red := Ui.error_color().to_html()
	var lines := PackedStringArray()
	lines.append("[url=%s]%s[/url]" % [key(file, test_name), test_name])
	var failures: Array = result.get("failures", [])
	var details: Array = result.get("details", [])
	for index in failures.size():
		for text: String in str(failures[index]).split("\n"):
			lines.append("  [color=#%s]%s[/color]" % [red, escape(text)])
		var detail: Dictionary = details[index] if index < details.size() else {}
		if not (detail.has("file") and detail.has("line")):
			continue
		var at := "%s:%d" % [detail.file, detail.line]
		lines.append("    [url=line:%s]at %s[/url]" % [at, escape(at)])
		if detail.has("code"):
			lines.append("    [color=#%s]%d |[/color] %s" % [dim, detail.line, escape(detail.code)])
		var pairs := PackedStringArray()
		for local: Dictionary in detail.get("locals", []):
			pairs.append("%s = %s" % [local.name, local.value])
		if not pairs.is_empty():
			lines.append("    [color=#%s]%s[/color]" % [dim, escape(", ".join(pairs))])
	return lines


## Whether a failure of `result` says where, without the test's variables:
## Godot reads them only with Always Track Local Variables on.
static func lacks_locals(result: Dictionary) -> bool:
	return result.get("details", []).any(
		func(detail: Dictionary) -> bool:
			return detail.has("line") and detail.get("locals", []).is_empty()
	)


## `text` shown as it is in a RichTextLabel, not read as BBCode.
static func escape(text: String) -> String:
	return text.replace("[", "[lb]")


## One line on the suite of `tests` (file -> [test name]) and their
## `results` (key -> {status, stale}), for the toolbar's tooltip.
static func summary(tests: Dictionary, results: Dictionary) -> String:
	var counts := { "passed": 0, "failed": 0, "not run": 0 }
	var changed := 0
	var total := 0
	for file: String in tests:
		for test_name: String in tests[file]:
			total += 1
			var result: Dictionary = results.get(key(file, test_name), {})
			match result.get("status", STATUS_IDLE):
				STATUS_PASS, STATUS_SKIP:
					counts.passed += 1
				STATUS_FAIL, STATUS_ERROR:
					counts.failed += 1
				_:
					counts["not run"] += 1
			if result.get("stale", false):
				changed += 1
	var text := "%d tests: %d passed, %d failed, %d not run" % [
		total, counts.passed, counts.failed, counts["not run"],
	]
	if changed > 0:
		text += "\n%d to run again: their scripts changed since they ran" % changed
	return text


## Wide enough for the longest duration, at the editor's scale: the Time
## column.
static func time_width() -> int:
	var size := EditorInterface.get_editor_theme().get_font_size("main_size", "EditorFonts")
	var width := Ui.mono_font().get_string_size("00 m 00 s", HORIZONTAL_ALIGNMENT_LEFT, -1, size).x
	return int(width + 24 * EditorInterface.get_editor_scale())


## Whether `targets` ("res://" folders, scripts, or "file::test") hold a test.
static func in_targets(file: String, test_name: String, targets: PackedStringArray) -> bool:
	for target in targets:
		var target_file := target.get_slice("::", 0)
		var only := target.get_slice("::", 1) if "::" in target else ""
		var folder := target_file.trim_suffix("/") + "/"
		if (file == target_file or file.begins_with(folder)) and (only == "" or only == test_name):
			return true
	return false
