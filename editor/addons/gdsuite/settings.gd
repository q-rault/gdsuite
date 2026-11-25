@tool
extends RefCounted
## The addon's settings. Personal preferences live in Editor Settings, under
## GDSuite. A project can set the same choices for everyone in the [editor]
## section of gdsuite.toml, which wins over the personal setting.

## Whether Check also fixes what can be fixed.
const AUTOFIX := "gdsuite/check/autofix"
## What a check covers.
const CHECK_FORMAT := "gdsuite/check/format"
const CHECK_LINT := "gdsuite/check/lint"
const CHECK_COMPILE := "gdsuite/check/compile"
const CHECK_HEALTH := "gdsuite/check/health"
## What saving a file does.
const CHECK_ON_SAVE := "gdsuite/on_save/check"
const AUTOFIX_ON_SAVE := "gdsuite/on_save/autofix"
const TEST_ON_SAVE := "gdsuite/on_save/test"
const SHOW_IN_GUTTER := "gdsuite/problems/show_in_gutter"
## The quick buttons of the editor's top bar.
const TOP_BAR_CHECK := "gdsuite/top_bar/check"
const TOP_BAR_TESTS := "gdsuite/top_bar/tests"
const PROGRAM := "gdsuite/program"

const DEFAULTS := {
	AUTOFIX: true,
	CHECK_FORMAT: true,
	CHECK_LINT: true,
	CHECK_COMPILE: true,
	CHECK_HEALTH: true,
	CHECK_ON_SAVE: true,
	AUTOFIX_ON_SAVE: false,
	TEST_ON_SAVE: false,
	SHOW_IN_GUTTER: true,
	TOP_BAR_CHECK: true,
	TOP_BAR_TESTS: true,
	PROGRAM: "",
}

## Editor setting -> key of gdsuite.toml's [editor] section.
const PROJECT_KEYS := {
	AUTOFIX: "autofix",
	CHECK_FORMAT: "check_format",
	CHECK_LINT: "check_lint",
	CHECK_COMPILE: "check_compile",
	CHECK_HEALTH: "check_health",
	CHECK_ON_SAVE: "check_on_save",
	AUTOFIX_ON_SAVE: "autofix_on_save",
	TEST_ON_SAVE: "test_on_save",
}

const LABELS := {
	AUTOFIX: "Autofix",
	CHECK_FORMAT: "Format",
	CHECK_LINT: "Lint",
	CHECK_COMPILE: "Compile",
	CHECK_HEALTH: "Health",
	CHECK_ON_SAVE: "Check",
	AUTOFIX_ON_SAVE: "Autofix",
	TEST_ON_SAVE: "Run the affected tests",
	SHOW_IN_GUTTER: "Problems in the script editor",
	TOP_BAR_CHECK: "Check in the top bar",
	TOP_BAR_TESTS: "Test all in the top bar",
}

const TOOLTIPS := {
	AUTOFIX: "Check formats the scripts, and applies the lint fixes that are safe",
	CHECK_FORMAT: "Scripts that formatting would change",
	CHECK_LINT: "The linter's rules",
	CHECK_COMPILE: "Godot's compiler: GDScript and shader errors, Godot's warnings, missing files",
	CHECK_HEALTH: "Code health: scripts too big or too tangled, and how they depend on each other",
	CHECK_ON_SAVE: "Saving a script checks it again, and the project a moment later",
	AUTOFIX_ON_SAVE: "Saving a script formats it, and applies the lint fixes that are safe",
	TEST_ON_SAVE: "Saving a script, a scene or a resource runs the tests it can affect",
	SHOW_IN_GUTTER: "Marks the lines with a problem in the script editor",
	TOP_BAR_CHECK: "The Check icon in the editor's top bar, which shows where the project stands",
	TOP_BAR_TESTS: "The flask in the editor's top bar, which shows where the tests stand",
}

## What these settings were before gdsuite 0.8: setting -> its old settings,
## and its old keys of [editor]. On when one of the old ones is.
const OLD_SETTINGS := {
	CHECK_ON_SAVE: ["gdsuite/on_save/lint", "gdsuite/on_save/type_check"],
	AUTOFIX_ON_SAVE: ["gdsuite/on_save/format"],
	TEST_ON_SAVE: ["gdsuite/on_save/run_tests"],
}
const OLD_PROJECT_KEYS := {
	CHECK_ON_SAVE: ["lint_on_save", "type_check_on_save"],
	AUTOFIX_ON_SAVE: ["format_on_save"],
	TEST_ON_SAVE: ["run_tests_on_save"],
}

## The project's [editor] section, read from gdsuite.toml by the plugin.
static var project_values := {}


static func register() -> void:
	var settings := EditorInterface.get_editor_settings()
	for key: String in OLD_SETTINGS:
		_replace_old(settings, key)
	for key: String in DEFAULTS:
		if not settings.has_setting(key):
			settings.set_setting(key, DEFAULTS[key])
		settings.set_initial_value(key, DEFAULTS[key], false)
	settings.add_property_info({
		"name": PROGRAM,
		"type": TYPE_STRING,
		"hint": PROPERTY_HINT_GLOBAL_FILE,
		"hint_string": "",
	})


## Gives `key` the value of the settings it replaces, and removes them. One
## left at its default isn't kept in Editor Settings, so it isn't found.
static func _replace_old(settings: EditorSettings, key: String) -> void:
	var on := false
	var found := false
	for old: String in OLD_SETTINGS[key]:
		if settings.has_setting(old):
			found = true
			on = on or bool(settings.get_setting(old))
			settings.erase(old)
		else:
			on = on or DEFAULTS[key]
	if found and not settings.has_setting(key):
		settings.set_setting(key, on)


static func get_personal(key: String) -> Variant:
	var settings := EditorInterface.get_editor_settings()
	if settings.has_setting(key):
		return settings.get_setting(key)
	return DEFAULTS.get(key)


static func set_personal(key: String, value: Variant) -> void:
	EditorInterface.get_editor_settings().set_setting(key, value)


## Whether gdsuite.toml decides this setting for everyone.
static func is_set_by_project(key: String) -> bool:
	return _project_value(key) != null


## The value in effect: the project's, or else yours.
static func value(key: String) -> Variant:
	var project: Variant = _project_value(key)
	return get_personal(key) if project == null else project


## The value [editor] gives, under its name or an old one, or null.
static func _project_value(key: String) -> Variant:
	if not PROJECT_KEYS.has(key):
		return null
	if project_values.has(PROJECT_KEYS[key]):
		return project_values[PROJECT_KEYS[key]]
	var old: Array = OLD_PROJECT_KEYS.get(key, [])
	old = old.filter(func(name: String) -> bool: return project_values.has(name))
	if old.is_empty():
		return null
	return old.any(func(name: String) -> bool: return bool(project_values[name]))
