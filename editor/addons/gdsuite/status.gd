@tool
## What Check and the tests show, in the top bar, on the docks' buttons and
## on the docks' tabs: two things apart. The result, what the last run found
## (nothing yet, fine, warnings, errors), and its freshness: whether it still
## holds for the code as it is now, or a run is going.
##
## The result is said by colour and by shape, for those who don't tell the
## colours apart: a badge with a check, a "!" or a cross, Godot's own status
## icons. Freshness is said by fading: a result from before the last changes
## is faded, as the docks fade problems and tests out of date. A run shows a
## spinner, in the colour of the editor's text, in place of the badge.

const Ui = preload("ui.gd")
const Problem = preload("problem.gd")

const RESULT_NONE := "none"
const RESULT_OK := "ok"
const RESULT_WARNING := "warning"
const RESULT_ERROR := "error"

const FRESH := "fresh"
const STALE := "stale"
const RUNNING := "running"

## How faded a result from before the last changes is.
const STALE_ALPHA := 0.45
## How faded the result a run will replace is: less than a stale one, so
## that the icon still reads behind the spinner.
const RUNNING_ALPHA := 0.75
const SPINNER_DOTS := 8
## How bright the spinner's turning dot and the two behind it are.
const SPINNER_TRAIL := [1.0, 0.75, 0.55]
## How bright its other dots are.
const SPINNER_DIM := 0.3
## The badge's size, as a share of the icon's.
const BADGE_SHARE := 0.62
## The editor icon of each result's badge.
const BADGES := {
	RESULT_OK: "StatusSuccess",
	RESULT_WARNING: "StatusWarning",
	RESULT_ERROR: "StatusError",
}

## The icons made, by what they show: making one reads every pixel.
static var _icons := {}


## The colour of `result`.
static func color(result: String) -> Color:
	var theme := EditorInterface.get_editor_theme()
	match result:
		RESULT_OK:
			return theme.get_color("success_color", "Editor")
		RESULT_WARNING:
			return Problem.severity_color("warning")
		RESULT_ERROR:
			return theme.get_color("error_color", "Editor")
	return Color(theme.get_color("font_color", "Editor"), 0.6)


## `base` (white, as Ui's icons are drawn) in the colour of `result`, with
## the badge of `result` at its bottom right, or a spinner showing `frame`
## while running; all faded when stale. A result of none has no badge: the
## grey alone says nothing is known yet.
static func icon(base: Texture2D, result: String, freshness: String, frame := 0) -> Texture2D:
	var running := freshness == RUNNING
	var key := "%d|%s|%s|%d|%s" % [
		base.get_rid().get_id(),
		result,
		freshness,
		frame % SPINNER_DOTS if running else 0,
		color(result).to_html(),
	]
	if _icons.has(key):
		return _icons[key]
	var image := _copy(base)
	if image.is_compressed():
		image.decompress()
	image.convert(Image.FORMAT_RGBA8)
	var tint := color(result)
	# A run fades what it will replace: the spinner says what goes on.
	var base_alpha := 1.0
	if freshness == STALE:
		base_alpha = STALE_ALPHA
	elif running:
		base_alpha = RUNNING_ALPHA
	for y in image.get_height():
		for x in image.get_width():
			var pixel := image.get_pixel(x, y)
			image.set_pixel(x, y, Color(tint, tint.a * pixel.a * pixel.v * base_alpha))
	var badge := badge_image(result, freshness, frame, image.get_width())
	if badge != null:
		_stamp(image, badge, STALE_ALPHA if freshness == STALE else 1.0)
	var texture := ImageTexture.create_from_image(image)
	_icons[key] = texture
	return texture


## The badge of `result`, `size` pixels wide, or a spinner frame while
## running; null for a result of none that doesn't run.
static func badge_image(result: String, freshness: String, frame: int, size: int) -> Image:
	var side := maxi(roundi(size * BADGE_SHARE), 4)
	if freshness == RUNNING:
		return _spinner(frame, side)
	var texture := Ui.icon(BADGES.get(result, ""))
	if texture == null:
		return null
	var badge := _copy(texture)
	if badge.is_compressed():
		badge.decompress()
	badge.convert(Image.FORMAT_RGBA8)
	badge.resize(side, side, Image.INTERPOLATE_LANCZOS)
	return badge


## A spinner of 8 dots, `side` pixels wide, in the colour of the editor's
## text, with dot `frame` bright and a short trail behind it. Godot's own
## Progress icons are dark grey, which doesn't read over the dark top bar.
static func _spinner(frame: int, side: int) -> Image:
	var text := EditorInterface.get_editor_theme().get_color("font_color", "Editor")
	var image := Image.create_empty(side, side, false, Image.FORMAT_RGBA8)
	var center := Vector2(side, side) / 2.0
	var dot := side * 0.13
	var around := side / 2.0 - dot - 0.5
	for index in SPINNER_DOTS:
		var behind := posmod(frame - index, SPINNER_DOTS)
		var bright := SPINNER_DIM
		if behind < SPINNER_TRAIL.size():
			bright = SPINNER_TRAIL[behind]
		var angle := TAU * index / SPINNER_DOTS - PI / 2.0
		var at := center + Vector2.from_angle(angle) * around
		for y in side:
			for x in side:
				# Edges smoothed over a pixel.
				var off := Vector2(x + 0.5, y + 0.5).distance_to(at)
				var cover := clampf(dot + 0.5 - off, 0.0, 1.0)
				if cover > 0.0:
					var alpha := maxf(image.get_pixel(x, y).a, cover * bright)
					image.set_pixel(x, y, Color(text, alpha))
	return image


## The badge alone, full size, for a dock's tab: the result's shape and
## colour, faded when stale, a spinner while running. Null for a result of
## none, which the tab shows as a grey dot.
static func tab_icon(result: String, freshness: String, frame := 0) -> Texture2D:
	if result == RESULT_NONE and freshness != RUNNING:
		return null
	var turned := frame % SPINNER_DOTS if freshness == RUNNING else 0
	var key := "tab|%s|%s|%d|%s" % [result, freshness, turned, color(result).to_html()]
	if _icons.has(key):
		return _icons[key]
	var size := roundi(16 * EditorInterface.get_editor_scale())
	var badge := badge_image(result, freshness, frame, roundi(size / BADGE_SHARE))
	if badge == null:
		return null
	if freshness == STALE:
		for y in badge.get_height():
			for x in badge.get_width():
				var pixel := badge.get_pixel(x, y)
				pixel.a *= STALE_ALPHA
				badge.set_pixel(x, y, pixel)
	var texture := ImageTexture.create_from_image(badge)
	_icons[key] = texture
	return texture


## A copy of the image of `texture`, to change: get_image() can give the
## texture's own image, which other icons, and the editor, draw from.
static func _copy(texture: Texture2D) -> Image:
	return texture.get_image().duplicate() as Image


## Draws `badge` over the bottom right of `image`, with a clear ring around
## it so it reads over the icon.
static func _stamp(image: Image, badge: Image, alpha: float) -> void:
	var side := badge.get_width()
	var origin := Vector2i(image.get_width() - side, image.get_height() - side)
	var center := Vector2(origin) + Vector2(side, side) / 2.0
	var ring := side / 2.0 + maxf(1.0, EditorInterface.get_editor_scale())
	for y in image.get_height():
		for x in image.get_width():
			if Vector2(x + 0.5, y + 0.5).distance_to(center) <= ring:
				image.set_pixel(x, y, Color(0, 0, 0, 0))
	for y in side:
		for x in side:
			var pixel := badge.get_pixel(x, y)
			pixel.a *= alpha
			var at := origin + Vector2i(x, y)
			var under := image.get_pixelv(at)
			image.set_pixelv(at, under.blend(pixel))
