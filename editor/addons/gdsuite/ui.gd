@tool
## The look the Problems and Tests docks share: their toolbar toggles, the
## counts on them, and how their lists are drawn.

## The lists' text, a little smaller than the editor's, for compact rows.
const FONT_SCALE := 0.9
## A toolbar toggle that is off.
const OFF_ALPHA := 0.4
## Secondary text in a row (a location already shown above, a duration).
const FADED_ALPHA := 0.55
## Height of the details below a list: a few lines, and a row of buttons.
const DETAILS_HEIGHT := 110


## A toolbar toggle: on, it looks like any button; off, it is faded. The
## theme's pressed colour would make "on" look like a state of its own.
static func toggle(
	bar: Container, text: String, tooltip: String, on: bool, toggled: Callable
) -> Button:
	var button := Button.new()
	button.toggle_mode = true
	button.button_pressed = on
	button.modulate.a = 1.0 if on else OFF_ALPHA
	button.flat = true
	button.text = text
	button.tooltip_text = tooltip
	var theme := EditorInterface.get_editor_theme()
	var font_color := theme.get_color("font_color", "Button")
	for state in ["font_pressed_color", "font_hover_pressed_color", "font_focus_color"]:
		button.add_theme_color_override(state, font_color)
	for state in ["icon_pressed_color", "icon_hover_pressed_color", "icon_focus_color"]:
		button.add_theme_color_override(state, Color.WHITE)
	button.toggled.connect(
		func(pressed: bool) -> void:
			button.modulate.a = 1.0 if pressed else OFF_ALPHA
			toggled.call()
	)
	bar.add_child(button)
	return button


## A filter of the toolbar: an optional icon, an optional name, then a count
## in a colour of its own (see `show_count`). With an icon, leave the name
## out: the tooltip says it, and the toolbar stays compact. The count's Label
## is the button's "count" meta; `add_count` adds another, `add_spinner` a
## spinner.
static func counted_toggle(
	bar: Container, icon: Texture2D, text: String, tooltip: String, toggled: Callable
) -> Button:
	var button := toggle(bar, "", tooltip, true, toggled)
	var box := HBoxContainer.new()
	box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var scale := EditorInterface.get_editor_scale()
	box.add_theme_constant_override("separation", roundi(3 * scale))
	if icon != null:
		var picture := TextureRect.new()
		picture.texture = icon
		picture.stretch_mode = TextureRect.STRETCH_KEEP_CENTERED
		picture.mouse_filter = Control.MOUSE_FILTER_IGNORE
		box.add_child(picture)
	if text != "":
		var name_label := Label.new()
		name_label.text = text
		name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
		box.add_child(name_label)
	button.add_child(box)
	button.set_meta("box", box)
	button.set_meta("count", add_count(button))
	var style := button.get_theme_stylebox("normal")
	var margin := Vector2(roundi(3 * scale), style.get_margin(SIDE_TOP))
	var fit := func() -> void:
		var size := box.get_combined_minimum_size()
		box.position = margin
		box.size = size
		button.custom_minimum_size = size + margin * 2
	box.minimum_size_changed.connect(fit)
	fit.call()
	return button


## Another count, after the others, on a `counted_toggle`.
static func add_count(button: Button) -> Label:
	var count := Label.new()
	count.mouse_filter = Control.MOUSE_FILTER_IGNORE
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


## From a Tree's `gui_input`: a click on a row that has rows under it (a
## script), past its arrow, folds or unfolds it, as the arrow does. Not with
## Shift or Ctrl held: those select. A double-click opens the script and
## leaves it as it was.
static func fold_on_click(tree: Tree, event: InputEvent) -> void:
	var click := event as InputEventMouseButton
	if click == null or not click.pressed or click.button_index != MOUSE_BUTTON_LEFT:
		return
	if click.shift_pressed or click.ctrl_pressed or click.meta_pressed:
		return
	var item := tree.get_item_at_position(click.position)
	if item == null or item.get_child_count() == 0:
		return
	# The arrow folds it already: it is in the indent before the row's text.
	var depth := 0
	var parent := item.get_parent()
	while parent != null and parent != tree.get_root():
		depth += 1
		parent = parent.get_parent()
	if click.position.x < (depth + 1) * tree.get_theme_constant("item_margin"):
		return
	var fold := func() -> void: item.collapsed = not item.collapsed
	fold.call_deferred()


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


## Fills `bar` green, or red once a test failed.
static func paint_progress(bar: ProgressBar, failed: bool) -> void:
	var fill := StyleBoxFlat.new()
	fill.bg_color = error_color() if failed else success_color()
	bar.add_theme_stylebox_override("fill", fill)
