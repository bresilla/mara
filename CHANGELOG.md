# Changelog

## [0.1.0] - 2026-05-19

### <!-- 0 -->⛰️  Features

- Add map UI and new hierarchy row widget
- Render vector basemaps
- Add map module for geographic data visualization

## [0.0.2] - 2026-05-17

### <!-- 0 -->⛰️  Features

- Add option to opt-out shelves from toggle buttons
- Add ribbon buttons for shelf visibility toggles
- Add app menu to shell chrome
- Clarify `mara_core` scope and heritage
- Rename to mara from frost
- Introduce native borderless window chrome
- Add web-based egui_frost demo leveraging eframe
- Add RibbonAvoidance for UI layout
- Improve shelf container behavior and icons
- Adjust shelf padding for top-aligned ribbon UI
- Add `draggable` property to individual ribbon slot items
- Add dedicated canvas view to demo app
- Refactor shelf state and tests
- Reanchor ghost gap entry in source shelf
- Improve input validation and cleanup state management
- Implement container movement across shelves
- Unify RibbonSlot with featureful chrome
- Add Shelves: persistent docked UI regions
- Add borderless window drag, resize, and native controls
- Implement core app shell and view router
- Add structured theme contracts
- Refactor pane bodies to use typed builder
- Refactor themes into dedicated files
- Add custom ribbons for fullscreen mode
- Reorganize core crates into modules
- Refactor `corekit` into `frost_core` and standalone crates
- Remove scanline overlay feature
- Introduce game-style theme elements and attributes
- Add sophisticated noise and multi-channel plot nodes
- Implement sharp-zoom node graph
- Implement GAME-theme tabbed containers
- Add tabbed containers
- Add wgpu vulkan backend to egui_frost dev deps
- Re-add crate + port demo from bevy_frost
- Normal::show_raw + floating shim drops Pod indirection
- Floating_window_for_item shim + foldable section
- Legacy aliases + command_palette + context_menu
- Add new pod widget types for chips and keybindings
- Add code editor and node graph widgets
- Migrate bevy_frost demo to corekit
- Integrate frostcore widgets into Bevy Frost examples
- Add new widgets and improve UI responsiveness
- Add new core widgets and enhance Pod flexibility
- Improve container sizing and input handling
- Implement resizable containers in panes
- Add support for resizable pods
- Add customizable pod separators and input absorption
- Add custom debug inspector, pod content unit
- Update corner bracket snap animation
- Refactor corner tick painting responsiveness
- Implement user-resizable panes
- Enhance GAME theme folding and animation
- Improve container corner tick appearance
- Improve pane layout and accent color handling
- Add optional pastel accent toggle
- Implement drag-and-drop reordering for Pane2 containers
- Improve window settings and corner ticks
- Enhance corner tick animations and appearance
- Implement staggered fade-in for container sections
- Enhance pane transitions with animation
- Add chromatic aberration effect to pane titles
- Theme-driven container spacing for Sections
- Refactor pane sizing and container chrome calculations
- Improve container layout and stacking
- Add title icons to Normal container
- Enhance container titles with theme-specific features
- Migrate to plain egui layout in Pane2 and Normal
- Draw accent banner for normal panes
- Refine container and pane sizing logic
- Introduce a new text input widget
- Add normal container widget and F12 debug toggle
- Revamp pane sizing and layout
- Introduce corekit UI module and flex-based panes
- Enhance pane title layout for middle anchors
- Refine pane positioning and title rendering
- Refactor pane positioning and title rendering logic
- Improve pane layout and title strip rendering
- Refine pane title stripe rendering
- Refactor pane drawing to match floating panes
- Implement flex-based pane skeleton and demo
- Integrate bevy_glacial and refine UI animations
- Add optimizations for Bevy frost demo
- Optimize UI rendering performance
- Configure new scramble-decode text motion
- Add new visual accent and progress bar animations
- Add 12 CSS-inspired animated button styles
- Enhance UI with new telemetry pip and animations
- Make animation time theme-dependent
- Add symmetric insetting to folded sections
- Refactor border and separator rendering
- Add distinct font weight for titles and panel stripes
- Add font weight selection to theme panel
- Refine accent color adaptation and theme consistency
- Add full Dark/Light mode support to themes
- Add vendored `egui_flex` as `frostcore::flex`
- More urdf
- Refine UI aesthetics and animations across FrostCore
- Revamp foldable sections inFrost to support theming
- Enhance theme system with advanced styling options
- Introduce Fluent UI System Icons support and theming
- Improve pane section reordering and auto-folding
- Implement deferred trailing separator
- Add command palette, status bar, and search field
- Integrate egui-snarl and egui_code_editor into Frost
- Introduce workspace to split into core and platform
- Add code editor with maximize support
- Improve Snarl integration with maximize/restore
- Add node-graph panel with egui-snarl integration
- Add inline RGBA color picker to example
- Add inline color pickers to color widgets
- Add `Tree` widget for hierarchical lists and `Dropdown`
- Improve ribbon demo and floating panel behavior
- Add full declarative ribbon system
- Init
- Init

### <!-- 1 -->🐛 Bug Fixes

- Zero AccumulatedMouseScroll/Motion when over a pane
- Floating shim renders sections as Normal containers
- Dropdown_control 5-arg shape matches dropdown

### <!-- 3 -->📚 Documentation

- Document Mara architecture and workspace
- Add architecture documentation

### <!-- 7 -->⚙️ Miscellaneous Tasks

- Update Makefile for docs and release targets
- Remove extensive comments from flake.nix
- Simplify Direnv setup and Nix Flake
- Refactor display/backend detection to .envrc
- Configure `git-cliff` for changelog generation

