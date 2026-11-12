@tool
extends RefCounted
## The addon's settings. Personal preferences live in Editor Settings, under
## GDSuite. A project can set the same choices for everyone in the [editor]
## section of gdsuite.toml, which wins over the personal setting.

const FORMAT_ON_SAVE := "gdsuite/on_save/format"
const LINT_ON_SAVE := "gdsuite/on_save/lint"
const TYPE_CHECK_ON_SAVE := "gdsuite/on_save/type_check"
const SHOW_IN_GUTTER := "gdsuite/problems/show_in_gutter"
const PROGRAM := "gdsuite/program"

const DEFAULTS := {
	FORMAT_ON_SAVE: false,
	LINT_ON_SAVE: true,
	TYPE_CHECK_ON_SAVE: true,
	SHOW_IN_GUTTER: true,
	PROGRAM: "",
}

## Editor setting -> key of gdsuite.toml's [editor] section.
const PROJECT_KEYS := {
	FORMAT_ON_SAVE: "format_on_save",
	LINT_ON_SAVE: "lint_on_save",
	TYPE_CHECK_ON_SAVE: "type_check_on_save",
}

const LABELS := {
	FORMAT_ON_SAVE: "Format on save",
	LINT_ON_SAVE: "Lint on save",
	TYPE_CHECK_ON_SAVE: "Type check after saving",
	SHOW_IN_GUTTER: "Show problems in the script editor",
}

## The project's [editor] section, read from gdsuite.toml by the plugin.
static var project_values := {}


static func register() -> void:
	var settings := EditorInterface.get_editor_settings()
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


static func get_personal(key: String) -> Variant:
	var settings := EditorInterface.get_editor_settings()
	if settings.has_setting(key):
		return settings.get_setting(key)
	return DEFAULTS.get(key)


static func set_personal(key: String, value: Variant) -> void:
	EditorInterface.get_editor_settings().set_setting(key, value)


## Whether gdsuite.toml decides this setting for everyone.
static func is_set_by_project(key: String) -> bool:
	return PROJECT_KEYS.has(key) and project_values.has(PROJECT_KEYS[key])


## The value in effect: the project's, or else yours.
static func value(key: String) -> Variant:
	if is_set_by_project(key):
		return project_values[PROJECT_KEYS[key]]
	return get_personal(key)
