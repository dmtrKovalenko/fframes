# anamorphic-video

A clip stored with non-square pixels, drawn at its display size. `media/anamorphic.mp4` is
480×360 with a sample aspect ratio of 4:3, so players show it at 640×360. The red outline
marks where its circle lands when the clip is drawn at `display_width()` × `display_height()`
with `preserveAspectRatio="none"`. Drawn at `width()` × `height()` the circle would be an
ellipse inside it.

From the repository root, `just render anamorphic-video` writes the video to a file.
