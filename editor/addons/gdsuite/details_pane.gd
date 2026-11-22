@tool
extends SplitContainer
## A dock's list, and the details of what is selected in it, below or to the
## right of it. Buttons in their corner fold them to one line, which says what is
## selected, and move them to the other side. Both are kept for the project.

## Height below the list, or width beside it, the details start with.
const BELOW_SIZE := 110
const BESIDE_SIZE := 300

## The details: what the dock fills in.
var body: Control

## Project metadata keys: "<key>_folded" and "<key>_beside".
var _key: String
var _folded := false
var _beside := false
var _details: VBoxContainer
var _header: HBoxContainer
var _corner: HBoxContainer
var _summary: Label
var _side_button: Button
## The text whose first line the folded header shows.
var _text: RichTextLabel


## `list` takes the room the details leave; `content` is the details, and
## `text` the RichTextLabel in it to sum up when folded.
func _init(list: Control, content: Control, text: RichTextLabel, key: String) -> void:
	body = content
	_text = text
	_key = key
	size_flags_vertical = SIZE_EXPAND_FILL
	list.size_flags_vertical = SIZE_EXPAND_FILL
	list.size_flags_horizontal = SIZE_EXPAND_FILL
	add_child(list)

	_details = VBoxContainer.new()
	_details.add_theme_constant_override("separation", 0)
	add_child(_details)
	# Folded: one row, which says what is selected.
	_header = HBoxContainer.new()
	_details.add_child(_header)
	_header_button(_header, "GuiTreeArrowRight", "Show the details", _toggle_folded)
	_summary = Label.new()
	_summary.size_flags_horizontal = SIZE_EXPAND_FILL
	_summary.clip_text = true
	_summary.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	_summary.modulate.a = 0.55
	_summary.mouse_filter = Control.MOUSE_FILTER_STOP
	_summary.tooltip_text = "Show the details"
	_summary.gui_input.connect(_on_summary_input)
	_header.add_child(_summary)
	# Open: the details, and in their corner, buttons to fold or move them.
	var stack := MarginContainer.new()
	stack.size_flags_vertical = SIZE_EXPAND_FILL
	_details.add_child(stack)
	content.size_flags_vertical = SIZE_EXPAND_FILL
	stack.add_child(content)
	_corner = HBoxContainer.new()
	_corner.size_flags_horizontal = SIZE_SHRINK_END
	_corner.size_flags_vertical = SIZE_SHRINK_BEGIN
	_corner.add_theme_constant_override("separation", 0)
	stack.add_child(_corner)
	_side_button = _header_button(_corner, "", "", _toggle_side)
	_header_button(_corner, "GuiTreeArrowDown", "Fold the details to one line", _toggle_folded)

	var settings := EditorInterface.get_editor_settings()
	_folded = settings.get_project_metadata("gdsuite", _key + "_folded", false)
	_beside = settings.get_project_metadata("gdsuite", _key + "_beside", false)
	_apply()


## The folded header shows the first line of the details: call it once they
## change.
func refresh_summary() -> void:
	var first := _text.get_parsed_text().strip_edges().get_slice("\n", 0)
	_summary.text = first if first != "" else "Details"


## Folded, the details are one line under the list, wherever they are put.
func _apply() -> void:
	var scale := EditorInterface.get_editor_scale()
	vertical = _folded or not _beside
	collapsed = _folded
	dragger_visibility = DRAGGER_HIDDEN_COLLAPSED if _folded else DRAGGER_VISIBLE
	_header.visible = _folded
	_details.get_child(1).visible = not _folded
	_details.size_flags_horizontal = SIZE_FILL
	_details.size_flags_vertical = SIZE_FILL
	if _beside and not _folded:
		_details.custom_minimum_size = Vector2(BESIDE_SIZE * scale, 0)
	elif not _folded:
		_details.custom_minimum_size = Vector2(0, BELOW_SIZE * scale)
	else:
		_details.custom_minimum_size = Vector2.ZERO
	split_offset = 0
	_side_button.icon = _icon("ControlAlignBottomWide" if _beside else "ControlAlignRightWide")
	_side_button.tooltip_text = (
		"Show the details below the list" if _beside else "Show the details beside the list"
	)
	refresh_summary()


func _toggle_folded() -> void:
	_folded = not _folded
	EditorInterface.get_editor_settings().set_project_metadata("gdsuite", _key + "_folded", _folded)
	_apply()


func _toggle_side() -> void:
	_beside = not _beside
	EditorInterface.get_editor_settings().set_project_metadata("gdsuite", _key + "_beside", _beside)
	_apply()


func _on_summary_input(event: InputEvent) -> void:
	var click := event as InputEventMouseButton
	if _folded and click != null and click.pressed and click.button_index == MOUSE_BUTTON_LEFT:
		_toggle_folded()


func _header_button(
	row: HBoxContainer, icon_name: String, tooltip: String, pressed: Callable
) -> Button:
	var button := Button.new()
	button.flat = true
	button.icon = _icon(icon_name)
	button.focus_mode = Control.FOCUS_NONE
	button.tooltip_text = tooltip
	button.pressed.connect(pressed)
	row.add_child(button)
	return button


static func _icon(icon_name: String) -> Texture2D:
	var theme := EditorInterface.get_editor_theme()
	if not theme.has_icon(icon_name, "EditorIcons"):
		return null
	return theme.get_icon(icon_name, "EditorIcons")
