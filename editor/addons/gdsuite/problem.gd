@tool
## A problem, as the Problems dock, the script editor and the plugin show it:
## which tool found it, its marks and colours, and what it says. Nothing here
## holds state.

const Ui = preload("ui.gd")

const SOURCE_LINT := "lint"
const SOURCE_TYPES := "types"
const SOURCE_FORMAT := "format"
const SOURCES := [SOURCE_FORMAT, SOURCE_LINT, SOURCE_TYPES]

## The marks for errors and warnings, the same in the filters, the list and
## the script editor. Not "✖": Godot draws it from a font that ignores the
## text's colour.
const ERROR_MARK := "×"
const WARNING_MARK := "⚠"
## The × is a small glyph: drawn this much larger, it matches the ⚠.
const ERROR_MARK_SCALE := 1.45
## Warnings in yellow on a dark theme, as the Check icon of the top bar; a
## light theme keeps its own, darker, warning colour.
const WARNING_YELLOW := Color(1.0, 0.8, 0.25)
## How strongly the script editor tints a problem's line.
const TINT_ALPHA := 0.12
## The tool each source is, as the dock names it.
const TOOL_NAMES := { SOURCE_FORMAT: "format", SOURCE_LINT: "lint", SOURCE_TYPES: "compile" }
const TOOL_DESCRIPTIONS := {
	SOURCE_FORMAT: "Found by the formatter (gdsuite fmt)",
	SOURCE_LINT: "Found by the linter (gdsuite lint)",
	SOURCE_TYPES: "Found by Godot's compiler (gdsuite typecheck)",
}
## What Godot's errors are, as Godot gives them no name.
const UNNAMED_ERROR := (
	"Godot's compiler gives its errors no name. For a syntax error, only the first one of a "
	+ "script is reported: fix it and check again."
)
## How the dock words each kind of change gdtidy reports.
const FORMAT_CHANGES := {
	"spacing": "Spaces to add or remove",
	"blank-lines": "Blank lines to add or remove",
	"indentation": "Indentation to fix",
	"trailing-whitespace": "Trailing whitespace to remove",
	"line-wrapping": "Lines to wrap or join",
	"quotes": "Quotes to change",
	"other": "Code to reformat",
}
## Codes that only say "an error": the tool says as much.
const GENERIC_CODES := ["gdscript-error", "shader-error", "load-error", "gdscript-warning"]


## The colour of an error or a warning, in the dock and the script editor.
static func severity_color(severity: String) -> Color:
	var theme := EditorInterface.get_editor_theme()
	if severity == "error":
		return theme.get_color("error_color", "Editor")
	var settings := EditorInterface.get_editor_settings()
	var base: Color = settings.get_setting("interface/theme/base_color")
	if base.get_luminance() > 0.5:
		return theme.get_color("warning_color", "Editor")
	return WARNING_YELLOW


## "12" or "12:5": the line, and the column when the tool gives one.
static func line(problem: Dictionary) -> String:
	if int(problem.line) <= 0:
		return ""
	if int(problem.get("column", 0)) > 0:
		return "%d:%d" % [problem.line, problem.column]
	return str(problem.line)


## "integer-division · compile", or the tool alone when the code only says
## "error".
static func source_text(problem: Dictionary) -> String:
	var tool: String = TOOL_NAMES[problem.source]
	if problem.rule in GENERIC_CODES or problem.rule == "":
		return tool
	return "%s · %s" % [problem.rule, tool]


## What a problem's code means: its kind, code and tool.
static func code_tooltip(problem: Dictionary) -> String:
	var kind := "Error" if problem.severity == "error" else "Warning"
	return "%s %s\n%s" % [kind, problem.rule, code_meaning(problem)]


## What a problem's code means, in a sentence.
static func code_meaning(problem: Dictionary) -> String:
	if problem.source == SOURCE_LINT:
		return "A lint rule of gdsuite. Ignore it on this line, or turn it off for the project."
	if problem.source == SOURCE_FORMAT:
		return "Formatting would change this script."
	if problem.source != SOURCE_TYPES:
		return ""
	var meanings := {
		"gdscript-error": UNNAMED_ERROR,
		"shader-error": UNNAMED_ERROR,
		"load-error": "Godot could not load this script.",
		"missing-resource": TOOL_DESCRIPTIONS[SOURCE_TYPES] + ".",
		"unsafe-access": TOOL_DESCRIPTIONS[SOURCE_TYPES] + ".",
		"gdscript-warning": TOOL_DESCRIPTIONS[SOURCE_TYPES] + ".",
	}
	var setting: String = problem.rule.replace("-", "_").to_upper()
	return meanings.get(
		problem.rule,
		"Godot's warning %s: set its level in Project Settings → Debug → GDScript." % setting
	)


## `path:line:col`, as compilers print it.
static func where(problem: Dictionary) -> String:
	var path: String = problem.file.trim_prefix("res://")
	var at := line(problem)
	return path if at == "" else "%s:%s" % [path, at]


## All of `problem`, as BBCode: its severity and code, its tool and place,
## its message, and what its code means.
static func details(problem: Dictionary) -> String:
	var dim := Ui.faded(Ui.text_color()).to_html()
	var color := severity_color(problem.severity).to_html()
	var head := "[color=#%s]%s[%s][/color]  [color=#%s]%s · %s[/color]" % [
		color, problem.severity, escape(problem.rule), dim, TOOL_NAMES[problem.source],
		escape(where(problem)),
	]
	var meaning := code_meaning(problem)
	if problem.source == SOURCE_LINT:
		meaning += " [url=docs]Read about it.[/url]"
	return "%s\n%s\n[color=#%s]%s[/color]" % [head, escape(problem.message), dim, meaning]


## `text` shown as it is in a RichTextLabel, not read as BBCode.
static func escape(text: String) -> String:
	return text.replace("[", "[lb]")


## The findings of a `--report json` result, as the dock's problems.
static func from_report(report: Dictionary) -> Array:
	var problems := []
	for finding: Dictionary in report.get("findings", []):
		problems.append({
			"file": "res://" + str(finding.get("file", "")).replace("\\", "/").trim_prefix("./"),
			"line": int(finding.get("line", 1)),
			"column": int(finding.get("column", 1)),
			"severity": str(finding.get("severity", "warning")),
			"message": str(finding.get("message", "")),
			"rule": str(finding.get("rule", "")),
			"fixable": bool(finding.get("fixable", false)),
		})
	return problems


## What formatting would change, from gdtidy's `changes`: the first change
## says what, the rest are counted. "Spacing to fix on lines 12–14, and 3
## more places".
static func format_message(changes: Array) -> String:
	if changes.is_empty():
		return "Not formatted"
	var first: Dictionary = changes[0]
	var what: String = FORMAT_CHANGES.get(str(first.kind), FORMAT_CHANGES.other)
	var from := int(first.first)
	var to := int(first.last)
	var lines := "line %d" % from if from == to else "lines %d–%d" % [from, to]
	var text := "%s on %s" % [what, lines]
	if changes.size() == 2:
		text += ", and 1 more place"
	elif changes.size() > 2:
		text += ", and %d more places" % (changes.size() - 1)
	return text


## The Problems dock's status: "Checking…", or when the last check reported
## (`checked_msec`, 0 for never), in minutes: seconds would catch the eye.
static func status(checking: bool, checked_msec: int) -> String:
	if checking:
		return "Checking…"
	if checked_msec == 0:
		return ""
	var minutes := (Time.get_ticks_msec() - checked_msec) / 60000
	if minutes < 1:
		return "Checked <1 min ago"
	if minutes < 60:
		return "Checked %d min ago" % minutes
	return "Checked %d h ago" % (minutes / 60)
