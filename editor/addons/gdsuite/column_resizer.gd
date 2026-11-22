@tool
extends RefCounted
## Lets the user drag the edges of a Tree's fixed-width columns to resize
## them, which Godot's Tree does not do itself. The widths are kept per
## project, in the editor's project metadata.

## Emitted when a drag ends, with the column it resized.
signal resized(column: int)

const GRAB := 5.0
const MIN_WIDTH := 40

var _tree: Tree
var _key: String
## Column index -> the name its width is saved under; by default, the index.
var _names := {}
## The column whose width a drag changes, and the side its edge is on:
## +1 when the edge is the column's right one, -1 when it is its left one.
var _edges := {}
var _dragging := -1
var _drag_from := 0.0
var _width_from := 0


## Makes the fixed columns of `tree` resizable; `defaults` gives their
## widths by column index. `key` names the saved widths; `names` (column
## index -> name) saves them by name instead of by index, for columns that
## can be shown and hidden.
func attach(tree: Tree, key: String, defaults: Dictionary, names := {}) -> void:
	_tree = tree
	_key = key
	configure(defaults, names)
	if not tree.gui_input.is_connected(_on_gui_input):
		tree.gui_input.connect(_on_gui_input)


## Sets which columns are resizable again, after the tree's columns changed.
func configure(defaults: Dictionary, names := {}) -> void:
	_names = names
	_edges.clear()
	for column: int in defaults:
		var width: int = defaults[column]
		var saved := saved_width(column)
		if saved > 0:
			width = saved
		_tree.set_column_expand(column, false)
		_tree.set_column_custom_minimum_width(column, width)
		_tree.set_column_clip_content(column, true)
	for column in _tree.columns:
		if not _tree.is_column_expanding(column):
			continue
		# A fixed column left of the expanding one is resized from its right
		# edge; one right of it, from its left edge.
		for fixed: int in defaults:
			_edges[fixed] = 1 if fixed < column else -1


## The width the user gave `column` by dragging it, or -1.
func saved_width(column: int) -> int:
	var saved: Variant = _metadata().get_project_metadata("gdsuite", _key, {})
	var name: String = _names.get(column, str(column))
	if saved is Dictionary and saved.has(name):
		return maxi(int(saved[name]), MIN_WIDTH)
	return -1


## Forgets the widths the user gave: the columns size themselves again.
func reset() -> void:
	_metadata().set_project_metadata("gdsuite", _key, {})


func _on_gui_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion:
		if _dragging >= 0:
			var moved: float = (event.position.x - _drag_from) * _edges[_dragging]
			_tree.set_column_custom_minimum_width(
				_dragging, maxi(int(_width_from + moved), MIN_WIDTH)
			)
			_tree.accept_event()
			return
		var hovered: bool = _edge_at(event.position.x) >= 0
		_tree.mouse_default_cursor_shape = Control.CURSOR_HSIZE if hovered else Control.CURSOR_ARROW
	elif event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
		if event.pressed:
			var column := _edge_at(event.position.x)
			if column < 0:
				return
			_dragging = column
			_drag_from = event.position.x
			_width_from = _tree.get_column_width(column)
			_tree.accept_event()
		elif _dragging >= 0:
			var column := _dragging
			_dragging = -1
			_save()
			_tree.accept_event()
			resized.emit(column)


## The column whose edge is under `x`, or -1.
func _edge_at(x: float) -> int:
	var left := _tree.get_theme_stylebox("panel").get_margin(SIDE_LEFT)
	var starts := []
	var at := left
	for column in _tree.columns:
		starts.append(at)
		at += _tree.get_column_width(column)
	for column: int in _edges:
		var edge: float = starts[column]
		if _edges[column] > 0:
			edge += _tree.get_column_width(column)
		if absf(x - edge) <= GRAB:
			return column
	return -1


func _save() -> void:
	var saved: Variant = _metadata().get_project_metadata("gdsuite", _key, {})
	# Keeps the widths of columns that are hidden now.
	var widths: Dictionary = saved.duplicate() if saved is Dictionary else {}
	for column: int in _edges:
		widths[_names.get(column, str(column))] = _tree.get_column_width(column)
	_metadata().set_project_metadata("gdsuite", _key, widths)


func _metadata() -> EditorSettings:
	return EditorInterface.get_editor_settings()
