# AGENTS.md - AI Coding Agent Guidelines for fframes

This file provides guidelines for AI coding agents working on the fframes codebase.

## Project Overview

fframes is a Rust-based video generation framework that renders videos from SVG-based scene descriptions.

**Tech Stack:**

- **Core:** Rust (Edition 2024), Cargo workspace
- **Editor:** ReScript (compiles to JS), React 19, Tailwind CSS v4
- **WASM:** wasm-bindgen, wasm-pack for browser preview
- **Graphics:** Skia renderer, FFmpeg for encoding/decoding

## Build & Development Commands

### Task Runner: Just (justfile)

```bash
# Full repository setup (run first after clone)
just init-repo

# Build everything (cargo + editor)
just build

# Watch editor during development
just watch-editor

# Run example with live reload (editor + WASM hot reload)
just run <example-name>

# Render example to video file
just render <example-name>
```

### Rust Commands

```bash
# Build
cargo build
cargo build --release

# Lint (warnings treated as errors)
just clippy
# Or directly:
cargo clippy -- -D warnings -A clippy::option-map-unit-fn -A clippy::module_inception -A clippy::single-match -A clippy::single-range-in-vec-init

# Run all tests
just test
# Or:
cargo test
cargo test -p fframes_test_utils --no-default-features

# Run a single test
cargo test <test_name>
cargo test -p <package> <test_name>

# Run tests in specific package
cargo test -p fframes
cargo test -p fframes-media

# Check WASM compilation for examples
just check-wasm <example-name>
just check-examples  # checks all examples
```

### Editor (ReScript/TypeScript) Commands

```bash
cd fframes-editor

# Build ReScript and bundle
yarn build

# Development mode with watch
yarn dev

# Clean ReScript build
yarn rescript:clean
```

### Package Management

```bash
# Lint package.json consistency
yarn syncpack lint

# Format JS/TS files
yarn prettier --write .
```

## Code Style Guidelines

### Rust Conventions

**Imports:**

- Group imports: std lib first, then external crates, then local modules
- Use `use crate::` for internal module imports
- Re-export public API through `lib.rs` using `pub use module::*;`

```rust
use std::{error::Error, fmt};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::{Color, Duration, Frame, Svgr};
```

**Error Handling:**

- Define custom errors as enums in `error.rs` modules
- Implement `std::error::Error` and `fmt::Display` for error types
- Use `type Result<T> = std::result::Result<T, CustomError>;` pattern
- Implement `From` traits for error conversion

```rust
#[derive(Debug)]
pub enum FFramesError {
    UserError(String),
    MediaError(crate::media::FFramesMediaError),
}

impl From<SomeError> for FFramesError {
    fn from(err: SomeError) -> Self { ... }
}
```

**Naming:**

- Types: `PascalCase` (e.g., `AnimationRuntime`, `KeyFrame`)
- Functions/methods: `snake_case` (e.g., `render_frame`, `get_duration`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `BACKGROUND_COLOR`)
- Traits: `PascalCase`, often adjectives (e.g., `Animatable`, `Sync`)

**Traits:**

- Use associated constants for video metadata: `const FPS`, `const WIDTH`, `const HEIGHT`
- Prefer `&self` methods over consuming self when possible
- Use lifetimes explicitly when returning references tied to self

```rust
pub trait Video: Sync + Sized {
    const FPS: usize;
    const WIDTH: usize;
    const HEIGHT: usize;

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a>;
}
```

**Documentation:**

- Use `///` doc comments for public APIs
- Include examples in doc comments with ```rust blocks
- Document panics, safety concerns, and performance considerations

**WASM Compatibility:**

- Use `#[cfg(not(target_arch = "wasm32"))]` for native-only code
- Use `#[cfg(target_arch = "wasm32")]` for WASM-specific code

### ReScript Conventions

**File Structure:**

- One component per file, named after component (e.g., `Editor.res`)
- Use `open` sparingly, prefer qualified access
- Place bindings in `src/bindings/` directory

**Components:**

```rescript
@genType.as("ComponentName") @react.component
let make = (~prop1: type1, ~prop2: type2) => {
  // hooks first
  let (state, setState) = React.useState(_ => initialValue)

  // effects
  React.useEffect0(() => { ... None })

  // render JSX
  <div className="tailwind-classes">
    {React.string("text")}
  </div>
}
```

**Naming:**

- Components: `PascalCase` files, lowercase `make` function
- Types: `lowerCamelCase` (ReScript convention)
- Variants: `PascalCase` (e.g., `MediaList.Grid`, `MediaList.List`)

**Styling:**

- Use Tailwind CSS classes via `className`
- Use `Cx.cx([...])` for conditional class concatenation

## Testing

**Rust Tests:**

- Unit tests in `#[cfg(test)] mod tests` blocks
- Integration tests in `/e2e/tests/`
- Visual regression tests compare rendered frames using `odiff`

**Running specific tests:**

```bash
# Run test by name
cargo test test_name

# Run tests matching pattern
cargo test animation

# Run with output
cargo test -- --nocapture
```

## Project Structure

```
/fframes           # Core rendering framework
/fframes-editor    # Web editor UI (ReScript + React)
/fframes-media     # Audio/video decoding, fonts, images
/fframes-skia-renderer  # GPU rendering backend
/fframes-test-utils     # Testing utilities
/svgr-macro        # SVG DSL procedural macro
/webvtt-parser     # Subtitle format parser
/e2e               # End-to-end visual tests
/examples          # Example video projects
```

## Common Patterns

**Creating a Video:**

```rust
impl Video for MyVideo {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> Duration<'_> { Duration::from_seconds(10.0) }
    fn audio(&self) -> AudioMap<'_> { AudioMap::empty() }
    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        svgr!(<svg>...</svg>)
    }
}
```

**Animations:**

```rust
let animation = timeline!(
    at 0.0, animate 0.0 => 1.0, Easing::EaseInOut,
    at 1.0 => 2.0, animate 1.0 => 0.0, Easing::Linear,
);
let value = animation.solve(frame.seconds());
```

## Important Notes

- Run `just clippy` before committing - warnings are errors
- Use `cargo fmt` for Rust formatting
- Use `yarn prettier --write .` for JS/TS/ReScript formatting
- Check WASM compilation with `just check-wasm <example>` when modifying editor bridge code
