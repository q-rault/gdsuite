@tool
extends Control
## The project's scripts and which uses which, from code health: the Graph
## view of the Problems dock. A script is left of those it uses. Its box has
## the colour and the number of its problems, and an arrow is red when it is
## part of a cycle. The whole project, or one script with what uses it and
## what it uses.
##
## Wheel: zoom. Drag: move. Click: select. Double-click: open.

## A script was selected; "" for none.
signal script_selected(path: String)
signal script_opened(path: String)

const Ui = preload("ui.gd")
const Problem = preload("problem.gd")

const BOX := Vector2(170, 22)
const GAP := Vector2(80, 6)
const MARGIN := 12.0
const MIN_ZOOM := 0.2
const MAX_ZOOM := 2.0
## Below this zoom the names can't be read: they are left out.
const NAMES_ZOOM := 0.45
## Passes that put each script near those it is linked to.
const ORDER_PASSES := 6
## How tall the layout of the whole project is at most, for its width: a
## column with more scripts goes on several, side by side.
const ASPECT := 0.45
const HELP := (
	"An arrow goes from a script to one it uses; a red one is part of a cycle. Click a script to "
	+ "see what it is linked to, double-click to open it. Wheel: zoom. Drag: move."
)
const NO_DATA := "The graph shows once Health has run: press Check."
const NO_SCRIPT := "No script of the project is open in the script editor."
const ALONE := "%s uses no other script, and none uses it."
const NONE_LINKED := "No script of the project uses another."
## How a script uses another, as the details say it, as the health report does.
const HOW := {
	"extends": "extends it",
	"preload": "preloads it",
	"load": "loads it",
	"class": "class name",
	"autoload": "autoload",
	"scene": "through a scene",
	"folder": "loads its folder",
	"path": "names its path",
}
const IN_CYCLE := (
	"Part of a dependency cycle: these scripts can't be changed or tested alone. Let one side "
	+ "emit a signal the other connects to, or move what they share to a script both use."
)

## "res://a.gd" -> [the scripts it uses], and -> [those that use it].
var _uses := {}
var _used_by := {}
## Every script of the project -> its number of lines.
var _scripts := {}
## [from, to] of the arrows within a cycle, as keys.
var _cycle_edges := {}
## [from, to] -> how `from` uses `to`: "extends", "class"... (see HOW).
var _how := {}
## path -> {errors, warnings}
var _counts := {}
## The script to show with its neighbours, or "" for the whole project.
var _focus := ""
var _whole := true
var _selected := ""
var _hovered := ""
## path -> Vector2, its place in the layout.
var _places := {}
var _offset := Vector2.ZERO
var _zoom := 1.0
var _dragging := false
## What says why nothing is shown.
var _message := NO_DATA
## Fit the layout in the view at the next draw: it has no size before.
var _fit_pending := true


func _init() -> void:
	size_flags_vertical = SIZE_EXPAND_FILL
	size_flags_horizontal = SIZE_EXPAND_FILL
	clip_contents = true
	focus_mode = FOCUS_CLICK
	resized.connect(_on_resized)


## Shows `graph`, as `gdsuite health --quick --report json` gives it.
func set_graph(graph: Dictionary) -> void:
	_uses.clear()
	_used_by.clear()
	_scripts.clear()
	_cycle_edges.clear()
	for script: Dictionary in graph.get("scripts", []):
		_scripts["res://" + str(script.path)] = int(script.get("lines", 0))
	_how.clear()
	for edge: Array in graph.get("edges", []):
		var from := "res://" + str(edge[0])
		var to := "res://" + str(edge[1])
		if edge.size() > 3:
			_how[[from, to]] = str(edge[3])
		if not _uses.has(from):
			_uses[from] = []
		if not _used_by.has(to):
			_used_by[to] = []
		_uses[from].append(to)
		_used_by[to].append(from)
	for cycle: Array in graph.get("cycles", []):
		var members := {}
		for path: Variant in cycle:
			members["res://" + str(path)] = true
		for from: String in members:
			for to: String in _uses.get(from, []):
				if members.has(to):
					_cycle_edges[[from, to]] = true
	_layout(false)


## path -> {errors, warnings}: the problems listed for each script.
func set_counts(counts: Dictionary) -> void:
	_counts = counts
	queue_redraw()


## Shows the whole project, or only `script` with what it is linked to.
func set_scope(whole: bool, script: String) -> void:
	var focus := "" if whole else script
	if whole == _whole and focus == _focus:
		return
	_whole = whole
	_focus = focus
	_layout(true)


func select(path: String) -> void:
	_selected = path
	queue_redraw()


## What `path` uses, and what uses it.
func uses(path: String) -> Array:
	return _uses.get(path, [])


func used_by(path: String) -> Array:
	return _used_by.get(path, [])


func lines_of(path: String) -> int:
	return _scripts.get(path, 0)


## The selected script, what it uses and what uses it, as BBCode.
func details() -> String:
	var dim := Ui.faded(Ui.text_color()).to_html()
	if _selected == "":
		return "[color=#%s]%s[/color]" % [dim, HELP]
	var lines := PackedStringArray([
		"%s  [color=#%s]%d lines[/color]" % [
			Problem.script_link(_selected), dim, lines_of(_selected),
		],
	])
	var linked := func(path: String, from: String, to: String) -> String:
		var how: String = HOW.get(_how.get([from, to], ""), "")
		var link := Problem.script_link(path)
		return link if how == "" else "%s [color=#%s](%s)[/color]" % [link, dim, how]
	var uses_links: Array = uses(_selected).map(
		func(to: String) -> String: return linked.call(to, _selected, to)
	)
	var used_links: Array = used_by(_selected).map(
		func(from: String) -> String: return linked.call(from, from, _selected)
	)
	for part: Array in [["Uses", uses_links], ["Used by", used_links]]:
		if not part[1].is_empty():
			lines.append("%s %s" % [part[0], ", ".join(part[1])])
	for edge: Array in _cycle_edges:
		if _selected in edge:
			lines.append("[color=#%s]%s[/color]" % [dim, IN_CYCLE])
			break
	return "\n".join(lines)


# --- Layout ------------------------------------------------------------------


func _layout(fit: bool) -> void:
	_places.clear()
	_message = ""
	var columns: Array = _project_columns() if _whole else _script_columns()
	for column in columns.size():
		var scripts: Array = columns[column]
		var height := scripts.size() * (BOX.y + GAP.y) - GAP.y
		for row in scripts.size():
			var x := column * (BOX.x + GAP.x)
			_places[scripts[row]] = Vector2(x, row * (BOX.y + GAP.y) - height / 2)
	if not _places.has(_selected):
		_selected = ""
	_fit_pending = _fit_pending or fit
	queue_redraw()


## The script, between those that use it and those it uses.
func _script_columns() -> Array:
	if _scripts.is_empty():
		_message = NO_DATA
		return []
	if not _scripts.has(_focus):
		_message = NO_SCRIPT
		return []
	var left: Array = used_by(_focus).duplicate()
	# In a cycle of two, a script is on both sides: once is enough.
	var right: Array = uses(_focus).filter(func(path: String) -> bool: return not path in left)
	if left.is_empty() and right.is_empty():
		_message = ALONE % _focus.get_file()
	left.sort()
	right.sort()
	return [left, [_focus], right].filter(func(column: Array) -> bool: return not column.is_empty())


## Every script linked to another, each in the first column right of all
## those that use it.
func _project_columns() -> Array:
	if _scripts.is_empty():
		_message = NO_DATA
		return []
	var linked := {}
	for path: String in _uses:
		linked[path] = true
	for path: String in _used_by:
		linked[path] = true
	if linked.is_empty():
		_message = NONE_LINKED
		return []
	var paths := linked.keys()
	paths.sort()
	var depth := {}
	var order := _by_dependency(paths)
	for path: String in order:
		depth[path] = 0
	var seen := {}
	for path: String in order:
		seen[path] = true
		for to: String in uses(path):
			# An arrow back to a script already placed closes a cycle.
			if not seen.has(to):
				depth[to] = maxi(depth[to], depth[path] + 1)
	var columns := []
	for path: String in paths:
		while columns.size() <= depth[path]:
			columns.append([])
		columns[depth[path]].append(path)
	_order_rows(columns)
	return _wrap(columns, paths.size())


## `columns`, with those taller than the layout should be split in several.
func _wrap(columns: Array, total: int) -> Array:
	# Rows and columns of boxes in the proportion the layout should have.
	var box_ratio := (BOX.y + GAP.y) / (BOX.x + GAP.x)
	var rows := maxi(ceili(sqrt(total * ASPECT / box_ratio)), 4)
	var wrapped := []
	for column: Array in columns:
		for start in range(0, column.size(), rows):
			wrapped.append(column.slice(start, start + rows))
	return wrapped


## `paths`, each before those it uses (except within a cycle).
func _by_dependency(paths: Array) -> Array:
	var done := {}
	var order := []
	for start: String in paths:
		if done.has(start):
			continue
		done[start] = true
		# [path, index of its next script to visit]: a stack rather than
		# recursion, which a long chain of scripts would make too deep.
		var stack := [[start, 0]]
		while not stack.is_empty():
			var top: Array = stack[-1]
			var next: Array = uses(top[0])
			if top[1] >= next.size():
				order.append(top[0])
				stack.pop_back()
				continue
			var to: String = next[top[1]]
			top[1] += 1
			if not done.has(to):
				done[to] = true
				stack.append([to, 0])
	order.reverse()
	return order


## Sorts each column so that a script is at the height of those it is linked
## to, which keeps the arrows short and apart.
func _order_rows(columns: Array) -> void:
	var height := {}
	for column: Array in columns:
		for row in column.size():
			height[column[row]] = row - column.size() / 2.0
	for pass_index in ORDER_PASSES:
		for column: Array in columns:
			var wanted := {}
			for path: String in column:
				var linked := uses(path) + used_by(path)
				var total := 0.0
				for other: String in linked:
					total += height[other]
				wanted[path] = total / linked.size()
			column.sort_custom(
				func(a: String, b: String) -> bool:
					return wanted[a] < wanted[b] if wanted[a] != wanted[b] else a < b
			)
			for row in column.size():
				height[column[row]] = row - column.size() / 2.0


## Shows all of the layout, no larger than its natural size.
func _fit() -> void:
	_fit_pending = false
	if _places.is_empty() or size.x <= 0 or size.y <= 0:
		return
	var bounds := Rect2(_places.values()[0], BOX)
	for path: String in _places:
		bounds = bounds.merge(Rect2(_places[path], BOX))
	var editor_scale := EditorInterface.get_editor_scale()
	var room := size - Vector2.ONE * 2 * MARGIN * editor_scale
	_zoom = minf(room.x / bounds.size.x, room.y / bounds.size.y)
	_zoom = clampf(_zoom, MIN_ZOOM, editor_scale)
	_offset = size / 2 - bounds.get_center() * _zoom


func _on_resized() -> void:
	_fit_pending = true
	queue_redraw()


# --- Drawing -----------------------------------------------------------------


func _draw() -> void:
	if _fit_pending:
		_fit()
	var font := get_theme_default_font()
	var font_size := Ui.font_size()
	if _message != "":
		var at := Vector2(MARGIN, size.y / 2) * EditorInterface.get_editor_scale()
		at.y = size.y / 2
		if _places.is_empty() or _places.size() == 1:
			at.y = MARGIN * 2 * EditorInterface.get_editor_scale()
		var dim := Ui.faded(Ui.text_color())
		draw_string(font, at, _message, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, dim)
	var lit := _hovered if _hovered != "" else _selected
	# The arrows of the script pointed at or selected, over the others.
	for pass_index in 2:
		for from: String in _uses:
			for to: String in _uses[from]:
				var is_lit := from == lit or to == lit
				if is_lit == (pass_index == 1) and _places.has(from) and _places.has(to):
					_draw_arrow(from, to, is_lit, lit != "")
	for path: String in _places:
		_draw_box(path, font, font_size, _is_faded(path, lit))


func _is_faded(path: String, lit: String) -> bool:
	if lit == "" or path == lit:
		return false
	return not (path in uses(lit) or path in used_by(lit))


func _draw_arrow(from: String, to: String, is_lit: bool, any_lit: bool) -> void:
	var start: Vector2 = _screen(_places[from] + Vector2(BOX.x, BOX.y / 2))
	var end: Vector2 = _screen(_places[to] + Vector2(0, BOX.y / 2))
	var color := Ui.text_color()
	color.a = 0.25
	if _cycle_edges.has([from, to]):
		color = Problem.severity_color("error")
	if is_lit:
		color.a = 1.0
	elif any_lit:
		color.a *= 0.3
	var reach := Vector2(maxf(absf(end.x - start.x) * 0.5, 30 * _zoom), 0)
	var points := PackedVector2Array()
	for step in 17:
		points.append(start.bezier_interpolate(start + reach, end - reach, end, step / 16.0))
	draw_polyline(points, color, 2.0 if is_lit else 1.0, true)
	var head := maxf(8 * _zoom, 4.0)
	draw_colored_polygon(
		PackedVector2Array([end, end + Vector2(-head, -head / 2), end + Vector2(-head, head / 2)]),
		color,
	)


func _draw_box(path: String, font: Font, font_size: int, faded: bool) -> void:
	var rect := Rect2(_screen(_places[path]), BOX * _zoom)
	var text_color := Ui.text_color()
	var count: Dictionary = _counts.get(path, {})
	var mark_color := text_color
	if count.get("errors", 0) > 0:
		mark_color = Problem.severity_color("error")
	elif count.get("warnings", 0) > 0:
		mark_color = Problem.severity_color("warning")
	var alpha := 0.3 if faded else 1.0
	var style := StyleBoxFlat.new()
	style.bg_color = get_theme_color("base_color", "Editor").lerp(text_color, 0.08)
	style.bg_color.a = alpha
	style.border_color = Color(mark_color, alpha * (0.25 if mark_color == text_color else 0.8))
	style.set_border_width_all(1)
	if path == _selected:
		style.border_color = get_theme_color("accent_color", "Editor")
		style.set_border_width_all(2)
	elif path == _focus:
		style.border_color = Color(text_color, 0.8)
	style.set_corner_radius_all(roundi(3 * _zoom))
	draw_style_box(style, rect)
	if _zoom < NAMES_ZOOM * EditorInterface.get_editor_scale():
		return
	var name_size := maxi(roundi(font_size * _zoom / EditorInterface.get_editor_scale()), 6)
	var pad := 6 * _zoom
	var middle := (font.get_ascent(name_size) - font.get_descent(name_size)) / 2
	var baseline := rect.get_center().y + middle
	var room := rect.size.x - pad * 2
	var total: int = count.get("errors", 0) + count.get("warnings", 0)
	if total > 0:
		var text := str(total)
		var width := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, name_size).x
		draw_string(
			font, Vector2(rect.end.x - pad - width, baseline), text, HORIZONTAL_ALIGNMENT_LEFT, -1,
			name_size, Color(mark_color, alpha)
		)
		room -= width + pad
	var line := TextLine.new()
	line.add_string(path.get_file(), font, name_size)
	line.width = room
	line.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	var top := rect.position.y + (rect.size.y - line.get_size().y) / 2
	line.draw(get_canvas_item(), Vector2(rect.position.x + pad, top), Color(text_color, alpha))


func _screen(at: Vector2) -> Vector2:
	return at * _zoom + _offset


# --- Mouse -------------------------------------------------------------------


func _gui_input(event: InputEvent) -> void:
	var motion := event as InputEventMouseMotion
	if motion != null:
		if _dragging:
			_offset += motion.relative
			queue_redraw()
		elif _script_at(motion.position) != _hovered:
			_hovered = _script_at(motion.position)
			queue_redraw()
		return
	var key := event as InputEventKey
	if key != null and key.pressed and key.keycode == KEY_ESCAPE:
		_select("")
		accept_event()
		return
	var click := event as InputEventMouseButton
	if click == null:
		return
	match click.button_index:
		MOUSE_BUTTON_WHEEL_UP, MOUSE_BUTTON_WHEEL_DOWN:
			if click.pressed:
				var factor := 1.15 if click.button_index == MOUSE_BUTTON_WHEEL_UP else 1 / 1.15
				_zoom_at(click.position, factor)
				accept_event()
		MOUSE_BUTTON_MIDDLE:
			_dragging = click.pressed
		MOUSE_BUTTON_LEFT:
			_on_left_button(click)


func _on_left_button(click: InputEventMouseButton) -> void:
	_dragging = false
	if not click.pressed:
		return
	var path := _script_at(click.position)
	if path != "":
		_select(path)
		if click.double_click:
			script_opened.emit(path)
	elif click.double_click:
		_fit()
		queue_redraw()
	else:
		_select("")
		_dragging = true


func _select(path: String) -> void:
	select(path)
	script_selected.emit(path)


func _zoom_at(at: Vector2, factor: float) -> void:
	var zoom := clampf(_zoom * factor, MIN_ZOOM, MAX_ZOOM * EditorInterface.get_editor_scale())
	_offset = at - (at - _offset) * zoom / _zoom
	_zoom = zoom
	queue_redraw()


func _script_at(at: Vector2) -> String:
	for path: String in _places:
		if Rect2(_screen(_places[path]), BOX * _zoom).has_point(at):
			return path
	return ""


func _notification(what: int) -> void:
	if what == NOTIFICATION_MOUSE_EXIT and _hovered != "":
		_hovered = ""
		queue_redraw()


func _get_tooltip(at_position: Vector2) -> String:
	var path := _script_at(at_position)
	if path == "":
		return ""
	var count: Dictionary = _counts.get(path, {})
	return "%s\n%d lines · uses %d · used by %d\n%d errors, %d warnings" % [
		path, lines_of(path), uses(path).size(), used_by(path).size(), count.get("errors", 0),
		count.get("warnings", 0),
	]
