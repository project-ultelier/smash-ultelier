Smash Ultelier is primarily an interface for smash plugins to the game's render loop but will hopefully grow into a tool for quicker reverse engineering of smash ultimate data structs.

Ideally multiple plugins should be able to talk to the same `ssbusync` without crashing but its
recommended to only have one plugin interface with the buffer settings.

Most of the time, you should treat `SmashUltelier` as a code library, not as a standalone plugin.
The default feature set is library-only; the ImGui console is opt-in with the `plugin` feature.

## Typical usage

You only need the shared `ssbusync` control API.
Import `ultelier` as a library:

```toml
[dependencies]
ultelier = { git = "https://github.com/BlankMauser/smash-ultelier.git", default-features = false, features = ["sync-guest"] }
```
Then make sure to install it in your main function.

```rust
#[skyline::main(name = "my-plugin")]
pub fn main() {
    let mut config = SsbuSyncConfig::vanilla();
    config.overclocker = true;
    ultelier::sync_guest::install(config);
}
```

Then use the re-exported guest API:

```rust
use ultelier::sync_guest::{self as sync, BufferMode, IndexMode};

pub fn enable_less_lag() {
    let _ = sync::set_buffer_mode(BufferMode::Double);
    let _ = sync::set_index_mode(IndexMode::OneBehind);
}
```

## Runtime buffer and index switching

This is the main reason to use the library.

Use `set_buffer_mode(...)` to switch between double buffer (less delay) and triple buffer (better performance). Vanilla is triple buffered.
Use `set_index_mode(...)` to switch between frame index modes. Vanilla is 2 frames behind. Set to 1 frame behind for less delay. Immediate mode (0 frames behind) is performance intensive and only emulators can run it, but you can shave off another frame of delay.
Use `set_vsync_enabled(...)` to toggle vsync. (disabled = less delay)
Use `set_render_opts_enabled(...)` to optimize smashed render and input polling loop (enabled = less delay). It is recommended to always enable this when changing buffer/index mode to something other than vanilla.

Basic toggle example:

```rust
use ultelier::sync_guest::{self as sync, BufferMode, IndexMode};

pub fn set_low_latency_mode(enabled: bool) {
    let buffer_target = match enabled {
        true => BufferMode::Double,
        false => BufferMode::Triple,
    };
    let index_target = match enabled {
        true => IndexMode::OneBehind, // use IndexMode::Immediate on emulators
        false => BufferMode::TwoBehind,
    };

    let _ = sync::set_vsync_enabled(!enabled);
    let _ = sync::set_render_opts_enabled(enabled);
    let _ = sync::set_buffer_mode(buffer_target);
    let _ = sync::set_index_mode(index_target);
}
```

You can also use the convenience helper:

```rust
let _ = ultelier::sync_guest::set_triple_buffer_enabled(true);
let _ = ultelier::sync_guest::set_triple_buffer_enabled(false);
```

That keeps the runtime in the correct mode for later triple/double transitions.

## Callbacks

`sync_guest::events` is the subscription API for runtime change notifications.

- `set_*` / `set_typed_*` returns `bool`: `true` if the callback was registered with the remote runtime, `false` means registration failed.
- The callback itself does not return a success value. It is just invoked when the state changes.

Event subscription example:

```rust
use ultelier::sync_guest::{self as sync, events, BufferMode, IndexBackend};

extern "C" fn on_buffer_mode_changed(mode: BufferMode) {
    skyline::println!("buffer mode changed to {:?}", mode);
}

extern "C" fn on_index_backend_changed(mode: IndexBackend) {
    skyline::println!("index backend changed to {:?}", mode);
}

pub fn subscribe_to_sync_callbacks() -> bool {
    events::set_typed_buffer_mode_changed(on_buffer_mode_changed)
        && events::set_typed_index_backend_changed(on_index_backend_changed)
}
```

To unsubscribe:

```rust
let _ = ultelier::sync_guest::events::clear_typed_buffer_mode_changed();
let _ = ultelier::sync_guest::events::clear_typed_index_backend_changed();
let _ = ultelier::sync_guest::events::clear_typed_vsync_changed();
let _ = ultelier::sync_guest::events::clear_typed_render_opts_changed();
```

## Resolution API

`sync_guest` also exposes helpers for default and runtime resolution control.

Default game resolution configuration:

- `set_default_game_resolution_level(level)` sets the baseline internal render level.
- `default_game_resolution_level()` reads the configured baseline as `Option<ResolutionLevel>`.
- `default_game_resolution()` reads the concrete `Resolution { width, height }` for the baseline.

Dynamic resolution control:

- `set_dynamic_resolution_enabled(enabled)` turns dynamic resolution on/off in the runtime.
- `dynamic_resolution_enabled()` reads whether dynamic resolution is currently enabled.
- `current_game_resolution()` reads the currently applied resolutions. If dynamic resolution is off, this returns the default resolution. Otherwise returns the currently applied dynamic resolution.
- `apparent_game_resolution()` reads the actual/effective resolution of the last presented frame.
- `push_dynamic_res_report(level)` requests a temporary dynamic-res level.
- `pop_dynamic_res_report(level)` removes one matching dynamic-res request.
- `clear_all_dynamic_res_report()` clears all pushed dynamic-res requests.

Example:

```rust
use ultelier::sync_guest::{self as sync, ResolutionLevel};

pub fn on_game_start() {
    let _ = sync::set_dynamic_resolution_enabled(true);
    let _ = sync::set_default_game_resolution_level(ResolutionLevel::Res1280x720);
}

pub fn on_game_frame() {
    if intensive_effect_started() {
        let _ = sync::push_dynamic_res_report(ResolutionLevel::Res1280x720);
    } else if intensive_effect_ended() {
        let _ = sync::pop_dynamic_res_report(ResolutionLevel::Res1280x720);
    }
}

pub fn on_game_end() {
    let _ = sync::set_dynamic_resolution_enabled(false);
    let _ = sync:clear_all_dynamic_res_report();
}
```
