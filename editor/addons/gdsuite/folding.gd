@tool
## Folding the scripts of a dock's list: a click on a script folds or unfolds
## it, and the toolbar's button folds them all, or, once all are folded,
## unfolds them all.


## From a Tree's `gui_input`: a click on a row that has rows under it (a
## script), past its arrow, folds or unfolds it, as the arrow does. Not with
## Shift or Ctrl held: those select. A double-click opens the script and
## leaves it as it was.
static func on_click(tree: Tree, event: InputEvent) -> void:
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


## Folds every script of `scripts` (TreeItems), or unfolds them all when all
## are folded. Returns whether they are folded now.
static func toggle_all(scripts: Array) -> bool:
	var fold := not all_folded(scripts)
	for item: TreeItem in scripts:
		item.collapsed = fold
	return fold


static func all_folded(scripts: Array) -> bool:
	var folded := func(item: TreeItem) -> bool: return item.collapsed
	return not scripts.is_empty() and scripts.all(folded)


## Shows on the toolbar's `button` what it does: fold every script, or unfold.
static func show_state(button: Button, scripts: Array) -> void:
	var folded := all_folded(scripts)
	var theme := EditorInterface.get_editor_theme()
	button.icon = theme.get_icon("ExpandTree" if folded else "CollapseTree", "EditorIcons")
	button.tooltip_text = "Unfold every script" if folded else "Fold every script"
