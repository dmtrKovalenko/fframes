type canvasSize = {
  width: float,
  height: float,
  scale: float,
  scaledWidth: float,
  scaledHeight: float,
  maxSceneWidth: float,
  frameToPxRatio: float,
  pxToFrameRation: float,
}

// Make sure to not change this from ints to float to enable preval of calculations

@inline
let timeline_margin_x = 64
@inline
let timeline_margin_y = 64
@inline
let scene_height_size = 120
@inline
let timeline_scenes_start_y = 24

let audio_height = scene_height_size / 2
