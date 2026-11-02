use fframes::Rect;

pub const FRAME_PADDING: u64 = 40;
pub const FRAME_WIDTH: u64 = 640;
pub const FRAME_HEIGHT: u64 = 1000;
pub const LEFT_FRAME: Rect = Rect::new(
    FRAME_PADDING as f32,
    FRAME_PADDING as f32,
    FRAME_WIDTH as f32,
    FRAME_HEIGHT as f32,
);
pub const RIGHT_FRAME: Rect = Rect::new(
    (FRAME_WIDTH + FRAME_PADDING * 2) as f32,
    FRAME_PADDING as f32,
    FRAME_WIDTH as f32,
    FRAME_HEIGHT as f32,
);

pub const CHAPTERS_START_X: u64 = FRAME_WIDTH * 2 + FRAME_PADDING * 3;
pub const CHAPTERS_START_Y: u64 = 145;
pub const CHAPTER_HEIGHT: u64 = 60;
pub const CHAPTER_WIDTH: u64 = 490;
pub const CHAPTER_PADDING: u64 = 15;
/// Horizontal inset of the chapter text inside its rectangle.
pub const CHAPTER_TEXT_PADDING: u64 = 10;
pub const CHAPTER_FONT_SIZE: usize = 26;
pub const CHAPTER_FONT_WEIGHT: u16 = 400;
pub const CHAPTER_FONT_FAMILY: &str = "Sofia Sans Semi Condensed";
