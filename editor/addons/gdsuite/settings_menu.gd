@tool
extends MenuButton
## The ⚙ of a dock's toolbar, the same in the Problems and Tests docks: the
## settings under a title each, then the dock's own items, then gdsuite.toml.

## A setting was turned on or off.
signal setting_toggled(key: String)
signal open_config_requested

const Settings = preload("settings.gd")
const Ui = preload("ui.gd")

const ID_CONFIG := 1000

## [[title, [setting, ...]], ...]
var _sections: Array
## Adds the dock's own items to the menu, after its settings.
var _extra: Callable
var _keys: Array = []


func _init(sections: Array, extra := Callable()) -> void:
	_sections = sections
	_extra = extra
	flat = true
	icon = Ui.icon("Tools")
	tooltip_text = "GDSuite settings"
	about_to_popup.connect(_fill)
	get_popup().id_pressed.connect(_on_pressed)


func _fill() -> void:
	var popup := get_popup()
	popup.clear()
	_keys.clear()
	for section: Array in _sections:
		popup.add_separator(section[0])
		for key: String in section[1]:
			var label: String = Settings.LABELS[key]
			var by_project := Settings.is_set_by_project(key)
			if by_project:
				label += " (set by this project)"
			var id := _keys.size()
			_keys.append(key)
			popup.add_check_item(label, id)
			var index := popup.get_item_index(id)
			popup.set_item_checked(index, bool(Settings.value(key)))
			popup.set_item_disabled(index, by_project)
			popup.set_item_tooltip(index, Settings.TOOLTIPS[key])
	if _extra.is_valid():
		popup.add_separator()
		_extra.call(popup)
	popup.add_separator()
	popup.add_item("Open gdsuite.toml", ID_CONFIG)


func _on_pressed(id: int) -> void:
	if id == ID_CONFIG:
		open_config_requested.emit()
	elif id < _keys.size():
		var key: String = _keys[id]
		Settings.set_personal(key, not bool(Settings.value(key)))
		setting_toggled.emit(key)
