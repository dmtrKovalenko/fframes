# anamorphic-video

A clip stored with non-square pixels. `media/anamorphic.mp4` is 480×360 with a sample aspect
ratio of 4:3, so players show it at 640×360. The decoder stretches it to that size, so
`into_image()`, `width()` and `height()` all report 640×360. The red outline marks where the
clip's circle lands at that size. Drawn at the coded 480×360 it would be an ellipse.

From the repository root, `just render anamorphic-video` writes the video to a file.
