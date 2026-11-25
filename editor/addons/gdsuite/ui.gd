@tool
## The look the Problems and Tests docks share: their toolbar toggles, the
## counts on them, and how their lists are drawn.

## The icons of Check and of Tests, the same in the editor's top bar and in
## the docks. Drawn white: the top bar colours them by state.
const CHECK_ICON := (
	'<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">'
	+ '<path d="M7.5 14.5H3v-13h6.5L13 5v3" fill="none" stroke="#fff" stroke-width="1.5" '
	+ 'stroke-linejoin="round"/><path d="M8.5 11.5l2.2 2.2 4-4.4" fill="none" stroke="#fff" '
	+ 'stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>'
)
const TESTS_ICON := (
	'<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">'
	+ '<path d="M5.5 1.5h5M6.5 1.5V6l-4 7.2a.9.9 0 0 0 .8 1.3h9.4a.9.9 0 0 0 .8-1.3L9.5 6V1.5" '
	+ 'fill="none" stroke="#fff" stroke-width="1.5" stroke-linecap="round" '
	+ 'stroke-linejoin="round"/><path d="M4.4 10h7.2l1.5 3H2.9z" fill="#fff"/></svg>'
)

## The lists' text, a little smaller than the editor's, for compact rows.
const FONT_SCALE := 0.9
## A toolbar toggle that is off.
const OFF_ALPHA := 0.4
## Secondary text in a row (a location already shown above, a duration).
const FADED_ALPHA := 0.55


## A filter of the toolbar, as the Output dock's: on, it has the pressed
## background; off, none, and faded. It shows an optional icon, an optional
## name, then a count in a colour of its own (see `show_count`). With an icon,
## leave the name out: the tooltip says it, and the toolbar stays compact. The
## count's Label is the button's "count" meta; `add_count` adds another,
## `add_spinner` a spinner.
static func counted_toggle(
	bar: Container, icon: Texture2D, text: String, tooltip: String, toggled: Callable
) -> Button:
	var button := Button.new()
	button.toggle_mode = true
	button.button_pressed = true
	button.theme_type_variation = &"EditorLogFilterButton"
	button.focus_mode = Control.FOCUS_NONE
	button.tooltip_text = tooltip
	bar.add_child(button)
	var box := HBoxContainer.new()
	box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var scale := EditorInterface.get_editor_scale()
	box.add_theme_constant_override("separation", roundi(2 * scale))
	if icon != null:
		var picture := TextureRect.new()
		picture.texture = icon
		picture.stretch_mode = TextureRect.STRETCH_KEEP_CENTERED
		picture.mouse_filter = Control.MOUSE_FILTER_IGNORE
		box.add_child(picture)
	if text != "":
		var name_label := _tight_label()
		name_label.text = text
		box.add_child(name_label)
	button.add_child(box)
	button.set_meta("box", box)
	button.set_meta("count", add_count(button))
	button.toggled.connect(
		func(pressed: bool) -> void:
			box.modulate.a = 1.0 if pressed else OFF_ALPHA
			toggled.call()
	)
	var margin := Vector2(roundi(5 * scale), roundi(2 * scale))
	# In the middle of the button, which the toolbar's row may make taller.
	var fit := func() -> void:
		var size := box.get_combined_minimum_size()
		box.size = size
		box.position = ((button.size - size) / 2).round()
		button.custom_minimum_size = size + margin * 2
	box.minimum_size_changed.connect(fit)
	button.resized.connect(fit)
	fit.call()
	return button


## One choice of a few, as one control: buttons side by side on a shared
## background, the chosen one pressed. `choices` are [text, icon name,
## tooltip]; `changed` is called with the index chosen.
static func switch(bar: Container, choices: Array, chosen: int, changed: Callable) -> Array[Button]:
	var frame := PanelContainer.new()
	var background := StyleBoxFlat.new()
	background.bg_color = text_color()
	background.bg_color.a = 0.06
	background.set_corner_radius_all(roundi(4 * EditorInterface.get_editor_scale()))
	frame.add_theme_stylebox_override("panel", background)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 0)
	frame.add_child(row)
	bar.add_child(frame)
	var group := ButtonGroup.new()
	var buttons: Array[Button] = []
	for index in choices.size():
		var choice: Array = choices[index]
		var button := Button.new()
		button.toggle_mode = true
		button.button_group = group
		button.button_pressed = index == chosen
		button.theme_type_variation = &"EditorLogFilterButton"
		button.focus_mode = Control.FOCUS_NONE
		button.text = choice[0]
		button.icon = icon(choice[1])
		button.tooltip_text = choice[2]
		button.pressed.connect(changed.bind(index))
		# The choice not made is faded, as a filter that is off.
		button.modulate.a = 1.0 if index == chosen else FADED_ALPHA
		button.toggled.connect(
			func(pressed: bool) -> void: button.modulate.a = 1.0 if pressed else FADED_ALPHA
		)
		row.add_child(button)
		buttons.append(button)
	return buttons


## Another count, after the others, on a `counted_toggle`.
static func add_count(button: Button) -> Label:
	var count := _tight_label()
	count.visible = false
	(button.get_meta("box") as HBoxContainer).add_child(count)
	return count


## A spinner before the name of a `counted_toggle`, hidden: `spin` turns it.
static func add_spinner(button: Button) -> TextureRect:
	var spinner := TextureRect.new()
	spinner.stretch_mode = TextureRect.STRETCH_KEEP_CENTERED
	spinner.mouse_filter = Control.MOUSE_FILTER_IGNORE
	spinner.visible = false
	var box: HBoxContainer = button.get_meta("box")
	box.add_child(spinner)
	box.move_child(spinner, 0)
	return spinner


## Shows the next frame of `spinner`, or hides it when `on` is false.
static func spin(spinner: TextureRect, on: bool, frame: int) -> void:
	spinner.visible = on
	if on:
		spinner.texture = EditorInterface.get_editor_theme().get_icon(
			"Progress%d" % (frame % 8 + 1), "EditorIcons"
		)


## Shows `number` in `color` on a count of `counted_toggle`, or nothing for 0.
static func show_count(label: Label, number: int, color: Color) -> void:
	label.text = str(number) if number > 0 else ""
	label.visible = number > 0
	label.add_theme_color_override("font_color", color)


## Sets a toolbar status. It takes the room the buttons leave and is cut
## when there is not enough: a long status never widens the dock, which would
## push the editor's other docks off the screen. Its tooltip has it all.
static func set_status(label: Label, text: String) -> void:
	label.text = text
	label.tooltip_text = text
	label.custom_minimum_size.x = 0


## The lists' compact rows.
static func style_tree(tree: Tree) -> void:
	tree.add_theme_font_size_override("font_size", font_size())
	tree.add_theme_constant_override(
		"v_separation", roundi(2 * EditorInterface.get_editor_scale())
	)


static func font_size() -> int:
	var main_size := EditorInterface.get_editor_theme().get_font_size("main_size", "EditorFonts")
	return roundi(main_size * FONT_SCALE)


static func text_color() -> Color:
	return EditorInterface.get_editor_theme().get_color("font_color", "Tree")


static func faded(color: Color) -> Color:
	color.a *= FADED_ALPHA
	return color


static func success_color() -> Color:
	return EditorInterface.get_editor_theme().get_color("success_color", "Editor")


## A dock's toolbar: on one line when the dock is wide enough, on more when
## not. A toolbar wider than its dock would widen the bottom panel, and push
## the editor's side docks off the screen.
static func toolbar() -> HFlowContainer:
	var bar := HFlowContainer.new()
	bar.add_theme_constant_override("v_separation", 0)
	return bar


static func error_color() -> Color:
	return EditorInterface.get_editor_theme().get_color("error_color", "Editor")


## A flat toolbar button: an icon (by its editor name, or none for ""), and a
## text, whose tooltip is the text when `tooltip` is empty.
static func button(
	bar: Container, icon_name: String, text: String, tooltip: String, pressed: Callable
) -> Button:
	var new_button := Button.new()
	new_button.flat = true
	new_button.icon = icon(icon_name)
	new_button.text = text
	new_button.tooltip_text = tooltip if tooltip != "" else text
	new_button.pressed.connect(pressed)
	bar.add_child(new_button)
	return new_button


static func svg_icon(svg: String) -> Texture2D:
	var image := Image.new()
	image.load_svg_from_string(svg, EditorInterface.get_editor_scale())
	return ImageTexture.create_from_image(image)


## An editor icon by name, or null when the editor has none of that name.
static func icon(icon_name: String) -> Texture2D:
	var theme := EditorInterface.get_editor_theme()
	if icon_name == "" or not theme.has_icon(icon_name, "EditorIcons"):
		return null
	return theme.get_icon(icon_name, "EditorIcons")


## The editor's monospace font, for durations and code.
static func mono_font() -> Font:
	return EditorInterface.get_editor_theme().get_font("source", "EditorFonts")


## The shortcut `name`, as the user set it in Editor Settings > Shortcuts,
## or else Alt+`keycode`, with Shift when `shift`.
static func register_shortcut(name: String, keycode: Key, shift: bool) -> Shortcut:
	var settings := EditorInterface.get_editor_settings()
	if settings.has_method("get_shortcut") and settings.call("has_shortcut", name):
		return settings.call("get_shortcut", name)
	var key := InputEventKey.new()
	key.keycode = keycode
	key.shift_pressed = shift
	key.alt_pressed = true
	var shortcut := Shortcut.new()
	shortcut.events = [key]
	# Godot 4.6 and newer list it in Editor Settings > Shortcuts, to rebind.
	if settings.has_method("add_shortcut"):
		settings.call("add_shortcut", name, shortcut)
	return shortcut


## A Label without the theme's padding: a count sits close to its name.
static func _tight_label() -> Label:
	var label := Label.new()
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.add_theme_stylebox_override("normal", StyleBoxEmpty.new())
	return label


## Fills `bar` green, or red once a test failed.
static func paint_progress(bar: ProgressBar, failed: bool) -> void:
	var fill := StyleBoxFlat.new()
	fill.bg_color = error_color() if failed else success_color()
	bar.add_theme_stylebox_override("fill", fill)
