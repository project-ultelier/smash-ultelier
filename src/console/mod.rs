use std::collections::VecDeque;
use std::ffi::c_char;
use std::ptr;
use std::sync::{Mutex, Once, OnceLock};

use imgui_api::bindings::*;
use nsuite::ninput;

use crate::sync_guest::{self, profile, BufferMode, EnvironmentFlags, IndexMode, OverclockProfile};

const MAX_LOG_LINES: usize = 256;
const COMMAND_BUFFER_LEN: usize = 256;
const AUTO_REFRESH_FRAMES: u32 = 30;

const WINDOW_TITLE: &[u8] = b"Ultelier Debug Console\0";
const LOG_CHILD_ID: &[u8] = b"ultelier_console_log\0";
const COMMAND_LABEL: &[u8] = b"##ultelier_console_command\0";
const COMMAND_HINT: &[u8] = b"help | refresh | render on | index onebehind\0";

const SECTION_RUNTIME: &[u8] = b"Runtime\0";
const SECTION_GRAPHICS: &[u8] = b"ngpu / NVN\0";
const SECTION_ACTIONS: &[u8] = b"Quick Actions\0";
const SECTION_COMMAND: &[u8] = b"Command\0";
const SECTION_LOG: &[u8] = b"Log\0";

const BUTTON_REFRESH: &[u8] = b"Refresh\0";
const BUTTON_REGISTER_CALLBACKS: &[u8] = b"Register Callbacks\0";
const BUTTON_CLEAR_LOG: &[u8] = b"Clear Log\0";
const BUTTON_RUN: &[u8] = b"Run\0";

const BUTTON_VSYNC_ON: &[u8] = b"VSync On\0";
const BUTTON_VSYNC_OFF: &[u8] = b"VSync Off\0";
const BUTTON_RENDER_OPTS_ON: &[u8] = b"Render Opts On\0";
const BUTTON_RENDER_OPTS_OFF: &[u8] = b"Render Opts Off\0";
const BUTTON_PACER_ON: &[u8] = b"Pacer On\0";
const BUTTON_PACER_OFF: &[u8] = b"Pacer Off\0";
const BUTTON_BUFFER_DOUBLE: &[u8] = b"Buffer Double\0";
const BUTTON_BUFFER_TRIPLE: &[u8] = b"Buffer Triple\0";
const BUTTON_INDEX_IMMEDIATE: &[u8] = b"Index Immediate\0";
const BUTTON_INDEX_ONE_BEHIND: &[u8] = b"Index OneBehind\0";
const BUTTON_INDEX_TWO_BEHIND: &[u8] = b"Index TwoBehind\0";

const CHECKBOX_SHOW_MOUSE: &[u8] = b"Show Mouse\0";
const CHECKBOX_AUTO_SCROLL: &[u8] = b"Auto Scroll\0";

static INSTALL_ONCE: Once = Once::new();
static CONSOLE_STATE: OnceLock<Mutex<ConsoleState>> = OnceLock::new();

#[derive(Debug, Clone, Copy, Default)]
struct RemoteSnapshot {
    remote_present: bool,
    runtime_status: Option<bool>,
    nstuff_status: Option<sync_guest::NsTuffStatus>,
    overclock_safe_profiles: Option<bool>,
    env_flags: Option<EnvironmentFlags>,
    overclock_profile: Option<OverclockProfile>,
    docked_profile: Option<profile::DockedProfile>,
    vsync_enabled: Option<bool>,
    render_opts_enabled: Option<bool>,
    pacer_enabled: Option<bool>,
    buffer_mode: Option<BufferMode>,
    index_mode: Option<IndexMode>,
}

#[derive(Debug)]
struct ConsoleState {
    snapshot: RemoteSnapshot,
    command_buffer: [u8; COMMAND_BUFFER_LEN],
    log_lines: VecDeque<String>,
    callbacks_registered: bool,
    visible: bool,
    show_mouse: bool,
    auto_scroll: bool,
    log_dirty: bool,
    frame_counter: u32,
    prev_toggle_combo_held: bool,
}

impl ConsoleState {
    fn new() -> Self {
        let mut state = Self {
            snapshot: RemoteSnapshot::default(),
            command_buffer: [0; COMMAND_BUFFER_LEN],
            log_lines: VecDeque::with_capacity(MAX_LOG_LINES),
            callbacks_registered: false,
            visible: false,
            show_mouse: true,
            auto_scroll: true,
            log_dirty: false,
            frame_counter: 0,
            prev_toggle_combo_held: false,
        };
        state.push_log("console ready: host requires imgui-smash and ssbusync to be loaded");
        state.push_log("toggle with ZL + ZR + DPad-Down");
        state
    }

    fn push_log(&mut self, line: impl Into<String>) {
        if self.log_lines.len() >= MAX_LOG_LINES {
            self.log_lines.pop_front();
        }
        self.log_lines.push_back(line.into());
        self.log_dirty = true;
    }
}

#[derive(Debug)]
enum Action {
    Refresh { verbose: bool },
    RegisterCallbacks,
    ClearLog,
    SetVsync(bool),
    SetRenderOpts(bool),
    SetPacer(bool),
    SetBufferMode(BufferMode),
    SetIndexMode(IndexMode),
    RunCommand(String),
}

fn state() -> &'static Mutex<ConsoleState> {
    CONSOLE_STATE.get_or_init(|| Mutex::new(ConsoleState::new()))
}

fn with_state<R>(f: impl FnOnce(&mut ConsoleState) -> R) -> R {
    let mut guard = state().lock().unwrap_or_else(|err| err.into_inner());
    f(&mut guard)
}

pub fn install() {
    INSTALL_ONCE.call_once(|| {
        with_state(|state| {
            state.push_log("installing imgui draw callback");
            state.push_log("initial snapshot deferred until first draw");
        });
        imgui_api::imgui_setup_context(setup_imgui_context);
        imgui_api::imgui_smash_add_on_draw_frame(draw as _);
    });
}

unsafe extern "C" fn setup_imgui_context(imgui_ctx: *mut u64) {
    igSetCurrentContext(imgui_ctx as _);
}

unsafe extern "C" fn draw() {
    let opened = poll_visibility_toggle();
    if opened {
        refresh_snapshot(true);
    }

    let mut actions = Vec::new();
    let show_mouse;

    {
        let mut state = state().lock().unwrap_or_else(|err| err.into_inner());
        state.frame_counter = state.frame_counter.wrapping_add(1);
        if state.frame_counter == 1 || state.frame_counter % AUTO_REFRESH_FRAMES == 0 {
            actions.push(Action::Refresh { verbose: false });
        }

        show_mouse = state.visible && state.show_mouse;

        imgui_api::imgui_smash_show_mouse(show_mouse);

        if !state.visible {
            return;
        }

        igSetNextWindowPos(
            ImVec2_c { x: 24.0, y: 24.0 },
            ImGuiCond_FirstUseEver as _,
            ImVec2_c { x: 0.0, y: 0.0 },
        );
        igSetNextWindowSize(ImVec2_c { x: 760.0, y: 620.0 }, ImGuiCond_FirstUseEver as _);
        igSetNextWindowBgAlpha(0.92);

        if !igBegin(
            WINDOW_TITLE.as_ptr() as _,
            ptr::null_mut(),
            ImGuiWindowFlags_NoCollapse as _,
        ) {
            igEnd();
            return;
        }

        render_runtime_section(&state);
        igSeparator();
        render_graphics_section(&state);
        igSeparator();
        render_quick_actions_section(&mut actions, &mut state);
        igSeparator();
        render_command_section(&mut actions, &mut state);
        igSeparator();
        render_log_section(&mut state);

        igEnd();
    }

    for action in actions {
        perform_action(action);
    }
}

unsafe fn render_runtime_section(state: &ConsoleState) {
    igSeparatorText(SECTION_RUNTIME.as_ptr() as _);
    text_line("ultelier debug console is running inside libultelier.nro");
    text_line("toggle: ZL + ZR + DPad-Down");
    text_line(&format!(
        "ssbusync symbols present: {}",
        on_off(state.snapshot.remote_present)
    ));
    text_line(&format!(
        "runtime initialized: {}",
        option_bool(state.snapshot.runtime_status)
    ));
    text_line(&format!(
        "overclock preset: {} | active profile: {} | docked slot: {}",
        option_overclock_preset(state.snapshot.overclock_safe_profiles),
        option_overclock_profile(state.snapshot.overclock_profile),
        option_docked_profile(state.snapshot.docked_profile)
    ));
    text_line(&format!(
        "nx-over: {} | title_match={} | poll={}ms | power profile={}",
        option_nstuff_enabled(state.snapshot.nstuff_status),
        option_nstuff_title_match(state.snapshot.nstuff_status),
        option_nstuff_poll_interval(state.snapshot.nstuff_status),
        option_nstuff_profile(state.snapshot.nstuff_status)
    ));
    text_line(&format!(
        "undervolt: {} | state: {} | cpu mode: {} | gpu mv: {}",
        option_uv_enabled(state.snapshot.nstuff_status),
        option_uv_state(state.snapshot.nstuff_status),
        option_uv_cpu_mode(state.snapshot.nstuff_status),
        option_uv_gpu_mv(state.snapshot.nstuff_status)
    ));
    text_line(&format!(
        "callbacks registered: {}",
        on_off(state.callbacks_registered)
    ));
    text_line(&format!(
        "vsync: {} | render_opts: {} | pacer: {} | buffer: {}",
        option_bool(state.snapshot.vsync_enabled),
        option_bool(state.snapshot.render_opts_enabled),
        option_bool(state.snapshot.pacer_enabled),
        option_buffer_mode(state.snapshot.buffer_mode)
    ));
    text_line(&format!(
        "index mode: {}",
        option_index_mode(state.snapshot.index_mode)
    ));
    text_line(&format!(
        "env flags: {}",
        state
            .snapshot
            .env_flags
            .map(|flags| format!("0x{:08X}", flags.bits()))
            .unwrap_or_else(|| "unavailable".to_string())
    ));
    text_line(&format!(
        "swapping_buffer={} swapping_index={} triple={} index_mode={}",
        option_flag(state.snapshot.env_flags, EnvironmentFlags::SWAPPING_BUFFER),
        option_flag(state.snapshot.env_flags, EnvironmentFlags::SWAPPING_INDEX),
        option_flag(state.snapshot.env_flags, EnvironmentFlags::TRIPLE_ENABLED),
        option_index_mode_field(state.snapshot.env_flags)
    ));
    text_line(&format!(
        "vsync={} render_opts={} pacer={} profiling={} slow_pacer_bias={} overclocker={}",
        option_flag(state.snapshot.env_flags, EnvironmentFlags::VSYNC_ENABLED),
        option_flag(
            state.snapshot.env_flags,
            EnvironmentFlags::RENDER_OPTS_ENABLED
        ),
        option_flag(state.snapshot.env_flags, EnvironmentFlags::PACER_ENABLED),
        option_flag(
            state.snapshot.env_flags,
            EnvironmentFlags::PROFILING_ENABLED
        ),
        option_flag(state.snapshot.env_flags, EnvironmentFlags::SLOW_PACER_BIAS),
        option_flag(state.snapshot.env_flags, EnvironmentFlags::OVERCLOCKER)
    ));
}

unsafe fn render_graphics_section(_state: &ConsoleState) {
    let _ = nsuite::ngpu::bootstrap::try_initialize_from_cached_device();

    igSeparatorText(SECTION_GRAPHICS.as_ptr() as _);
    text_line(&format!(
        "bootstrap active: {} | ngpu initialized: {}",
        on_off(nsuite::ngpu::bootstrap::bootstrap_active()),
        on_off(nsuite::ngpu::is_initialized())
    ));
    text_line(&format!(
        "device: {} | window: {}",
        option_ptr(nsuite::ngpu::bootstrap::cached_device()),
        option_ptr(nsuite::ngpu::bootstrap::cached_window())
    ));
    text_line(&format!(
        "queue: {} | present queue: {}",
        option_ptr(nsuite::ngpu::bootstrap::cached_queue()),
        option_ptr(nsuite::ngpu::bootstrap::cached_present_queue())
    ));
    text_line(&format!(
        "active texture index: {} | active texture: {}",
        nsuite::ngpu::bootstrap::cached_active_window_texture_index()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
        option_ptr(nsuite::ngpu::bootstrap::cached_active_window_texture())
    ));
    text_line(&format!(
        "driver api: {} | draw_texture support: {}",
        nsuite::ngpu::bootstrap::cached_driver_api_versions()
            .map(|(major, minor)| format!("{}.{}", major, minor))
            .unwrap_or_else(|| "unavailable".to_string()),
        nsuite::ngpu::bootstrap::cached_supports_draw_texture()
            .map(on_off)
            .unwrap_or("unknown")
    ));

    let mut queues = [0usize; 64];
    let queue_count = nsuite::ngpu::bootstrap::tracked_submit_queues_snapshot(&mut queues);
    let tracked: Vec<String> = queues
        .iter()
        .take(queue_count.min(3))
        .filter(|value| **value != 0)
        .map(|value| format!("0x{value:X}"))
        .collect();
    text_line(&format!(
        "tracked submit queues: {}{}",
        queue_count,
        if tracked.is_empty() {
            String::new()
        } else {
            format!(" [{}]", tracked.join(", "))
        }
    ));

    let mut textures = [ptr::null_mut(); 8];
    let texture_count = nsuite::ngpu::bootstrap::cached_window_textures_snapshot(&mut textures);
    let preview: Vec<String> = textures
        .iter()
        .take(texture_count.min(3))
        .filter(|value| !value.is_null())
        .map(|value| format!("{value:p}"))
        .collect();
    text_line(&format!(
        "cached window textures: {}{}",
        texture_count,
        if preview.is_empty() {
            String::new()
        } else {
            format!(" [{}]", preview.join(", "))
        }
    ));
}

unsafe fn render_quick_actions_section(actions: &mut Vec<Action>, state: &mut ConsoleState) {
    igSeparatorText(SECTION_ACTIONS.as_ptr() as _);
    if igButton(BUTTON_REFRESH.as_ptr() as _, zero_vec2()) {
        actions.push(Action::Refresh { verbose: true });
    }
    igSameLine(0.0, 8.0);
    if igButton(BUTTON_REGISTER_CALLBACKS.as_ptr() as _, zero_vec2()) {
        actions.push(Action::RegisterCallbacks);
    }
    igSameLine(0.0, 8.0);
    if igButton(BUTTON_CLEAR_LOG.as_ptr() as _, zero_vec2()) {
        actions.push(Action::ClearLog);
    }

    igCheckbox(CHECKBOX_SHOW_MOUSE.as_ptr() as _, &mut state.show_mouse);
    igSameLine(0.0, 16.0);
    igCheckbox(CHECKBOX_AUTO_SCROLL.as_ptr() as _, &mut state.auto_scroll);

    if igButton(BUTTON_VSYNC_ON.as_ptr() as _, zero_vec2()) {
        actions.push(Action::SetVsync(true));
    }
    igSameLine(0.0, 8.0);
    if igButton(BUTTON_VSYNC_OFF.as_ptr() as _, zero_vec2()) {
        actions.push(Action::SetVsync(false));
    }
    igSameLine(0.0, 16.0);
    if igButton(BUTTON_RENDER_OPTS_ON.as_ptr() as _, zero_vec2()) {
        actions.push(Action::SetRenderOpts(true));
    }
    igSameLine(0.0, 8.0);
    if igButton(BUTTON_RENDER_OPTS_OFF.as_ptr() as _, zero_vec2()) {
        actions.push(Action::SetRenderOpts(false));
    }
    igSameLine(0.0, 16.0);
    if igButton(BUTTON_PACER_ON.as_ptr() as _, zero_vec2()) {
        actions.push(Action::SetPacer(true));
    }
    igSameLine(0.0, 8.0);
    if igButton(BUTTON_PACER_OFF.as_ptr() as _, zero_vec2()) {
        actions.push(Action::SetPacer(false));
    }

    if igButton(BUTTON_BUFFER_DOUBLE.as_ptr() as _, zero_vec2()) {
        actions.push(Action::SetBufferMode(BufferMode::Double));
    }
    igSameLine(0.0, 8.0);
    if igButton(BUTTON_BUFFER_TRIPLE.as_ptr() as _, zero_vec2()) {
        actions.push(Action::SetBufferMode(BufferMode::Triple));
    }

    igSameLine(0.0, 16.0);
    if igButton(BUTTON_INDEX_IMMEDIATE.as_ptr() as _, zero_vec2()) {
        actions.push(Action::SetIndexMode(IndexMode::Immediate));
    }
    igSameLine(0.0, 8.0);
    if igButton(BUTTON_INDEX_ONE_BEHIND.as_ptr() as _, zero_vec2()) {
        actions.push(Action::SetIndexMode(IndexMode::OneBehind));
    }
    igSameLine(0.0, 8.0);
    if igButton(BUTTON_INDEX_TWO_BEHIND.as_ptr() as _, zero_vec2()) {
        actions.push(Action::SetIndexMode(IndexMode::TwoBehind));
    }
}

unsafe fn render_command_section(actions: &mut Vec<Action>, state: &mut ConsoleState) {
    igSeparatorText(SECTION_COMMAND.as_ptr() as _);
    text_line("commands: help, refresh, clear, mouse on/off, vsync on/off, render on/off, pacer on/off, buffer double/triple, index immediate/onebehind/twobehind");

    let submitted = igInputTextWithHint(
        COMMAND_LABEL.as_ptr() as _,
        COMMAND_HINT.as_ptr() as _,
        state.command_buffer.as_mut_ptr() as *mut c_char,
        state.command_buffer.len(),
        (ImGuiInputTextFlags_EnterReturnsTrue | ImGuiInputTextFlags_AutoSelectAll) as _,
        None,
        ptr::null_mut(),
    );

    igSameLine(0.0, 8.0);
    let clicked = igButton(BUTTON_RUN.as_ptr() as _, zero_vec2());
    if submitted || clicked {
        let command = read_command_buffer(&state.command_buffer);
        state.command_buffer.fill(0);
        if !command.is_empty() {
            actions.push(Action::RunCommand(command));
        }
    }
}

unsafe fn render_log_section(state: &mut ConsoleState) {
    igSeparatorText(SECTION_LOG.as_ptr() as _);
    if igBeginChild_Str(
        LOG_CHILD_ID.as_ptr() as _,
        ImVec2_c { x: 0.0, y: 140.0 },
        ImGuiChildFlags_Borders as _,
        0,
    ) {
        for line in &state.log_lines {
            text_line(line);
        }
        if state.auto_scroll && state.log_dirty {
            igSetScrollHereY(1.0);
        }
        state.log_dirty = false;
    }
    igEndChild();
}

fn perform_action(action: Action) {
    match action {
        Action::Refresh { verbose } => refresh_snapshot(verbose),
        Action::RegisterCallbacks => {
            maybe_register_callbacks(true);
            refresh_snapshot(false);
        }
        Action::ClearLog => {
            with_state(|state| {
                state.log_lines.clear();
                state.push_log("log cleared");
            });
        }
        Action::SetVsync(enabled) => {
            let result = sync_guest::set_vsync_enabled(enabled);
            log_guest_call("set_vsync_enabled", enabled, result);
            refresh_snapshot(false);
        }
        Action::SetRenderOpts(enabled) => {
            let result = sync_guest::set_render_opts_enabled(enabled);
            log_guest_call("set_render_opts_enabled", enabled, result);
            refresh_snapshot(false);
        }
        Action::SetPacer(enabled) => {
            let result = sync_guest::set_pacer_enabled(enabled);
            log_guest_call("set_pacer_enabled", enabled, result);
            refresh_snapshot(false);
        }
        Action::SetBufferMode(mode) => {
            let result = sync_guest::set_buffer_mode(mode);
            with_state(|state| {
                state.push_log(format!(
                    "set_buffer_mode({}) -> {}",
                    buffer_mode_name(mode),
                    guest_result(result)
                ));
            });
            refresh_snapshot(false);
        }
        Action::SetIndexMode(mode) => {
            let result = sync_guest::set_index_mode(mode);
            with_state(|state| {
                state.push_log(format!(
                    "set_index_mode({}) -> {}",
                    index_mode_name(mode),
                    guest_result(result)
                ));
            });
            refresh_snapshot(false);
        }
        Action::RunCommand(command) => run_command(command),
    }
}

unsafe fn poll_visibility_toggle() -> bool {
    ninput::CheckInputs();
    let held = ninput::FirstControllerWithAll(toggle_mask()).is_some();

    with_state(|state| {
        let rising_edge = held && !state.prev_toggle_combo_held;
        state.prev_toggle_combo_held = held;
        if !rising_edge {
            return false;
        }

        state.visible = !state.visible;
        state.push_log(format!("console visibility -> {}", on_off(state.visible)));
        state.visible
    })
}

fn run_command(command: String) {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return;
    }

    let normalized = trimmed.to_ascii_lowercase();
    if normalized == "clear" {
        perform_action(Action::ClearLog);
        return;
    }

    with_state(|state| {
        state.push_log(format!("> {trimmed}"));
    });

    let mut parts = normalized.split_whitespace();
    let Some(head) = parts.next() else {
        return;
    };

    match head {
        "help" => {
            with_state(|state| {
                state.push_log("help: refresh | clear | callbacks");
                state.push_log("help: mouse on/off");
                state.push_log("help: vsync on/off | render on/off | pacer on/off");
                state.push_log("help: buffer double/triple");
                state.push_log("help: index immediate/onebehind/twobehind");
            });
        }
        "refresh" | "status" => refresh_snapshot(true),
        "callbacks" => {
            maybe_register_callbacks(true);
            refresh_snapshot(false);
        }
        "mouse" => match parts.next() {
            Some("on") => with_state(|state| {
                state.show_mouse = true;
                state.push_log("show_mouse -> on");
            }),
            Some("off") => with_state(|state| {
                state.show_mouse = false;
                state.push_log("show_mouse -> off");
            }),
            _ => push_invalid_command("mouse expects 'on' or 'off'"),
        },
        "vsync" => match parts.next() {
            Some("on") => perform_action(Action::SetVsync(true)),
            Some("off") => perform_action(Action::SetVsync(false)),
            _ => push_invalid_command("vsync expects 'on' or 'off'"),
        },
        "render" | "renderopts" => match parts.next() {
            Some("on") => perform_action(Action::SetRenderOpts(true)),
            Some("off") => perform_action(Action::SetRenderOpts(false)),
            _ => push_invalid_command("render expects 'on' or 'off'"),
        },
        "pacer" => match parts.next() {
            Some("on") => perform_action(Action::SetPacer(true)),
            Some("off") => perform_action(Action::SetPacer(false)),
            _ => push_invalid_command("pacer expects 'on' or 'off'"),
        },
        "buffer" => match parts.next() {
            Some("double") => perform_action(Action::SetBufferMode(BufferMode::Double)),
            Some("triple") => perform_action(Action::SetBufferMode(BufferMode::Triple)),
            _ => push_invalid_command("buffer expects 'double' or 'triple'"),
        },
        "index" | "frame" => match parts.next() {
            Some("immediate") => perform_action(Action::SetIndexMode(IndexMode::Immediate)),
            Some("onebehind") | Some("double") => {
                perform_action(Action::SetIndexMode(IndexMode::OneBehind))
            }
            Some("twobehind") | Some("triple") | Some("vanilla") | Some("frozen") => {
                perform_action(Action::SetIndexMode(IndexMode::TwoBehind))
            }
            _ => push_invalid_command("index expects immediate/onebehind/twobehind"),
        },
        _ => push_invalid_command("unknown command; type 'help'"),
    }
}

fn push_invalid_command(message: &str) {
    with_state(|state| {
        state.push_log(format!("error: {message}"));
    });
}

fn log_guest_call(function_name: &str, enabled: bool, result: Option<bool>) {
    with_state(|state| {
        state.push_log(format!(
            "{function_name}({}) -> {}",
            on_off(enabled),
            guest_result(result)
        ));
    });
}

fn maybe_register_callbacks(verbose: bool) {
    if !sync_guest::remote_present() {
        with_state(|state| {
            state.callbacks_registered = false;
            if verbose {
                state.push_log("ssbusync symbols are not present; callback registration skipped");
            }
        });
        return;
    }

    let already_registered = with_state(|state| state.callbacks_registered);
    if already_registered {
        if verbose {
            with_state(|state| {
                state.push_log("callbacks already registered");
            });
        }
        return;
    }

    let vsync_ok = sync_guest::events::set_typed_vsync_changed(on_vsync_changed);
    let render_ok = sync_guest::events::set_typed_render_opts_changed(on_render_opts_changed);
    let buffer_ok = sync_guest::events::set_typed_buffer_mode_changed(on_buffer_mode_changed);
    let index_ok = sync_guest::events::set_typed_index_mode_changed(on_index_mode_changed);
    let ok = vsync_ok && render_ok && buffer_ok && index_ok;

    with_state(|state| {
        state.callbacks_registered = ok;
        state.push_log(if ok {
            "registered ssbusync typed callbacks".to_string()
        } else {
            "failed to register one or more ssbusync callbacks".to_string()
        });
    });
}

fn refresh_snapshot(verbose: bool) {
    maybe_register_callbacks(false);
    let remote_present = sync_guest::remote_present();
    let runtime_status = sync_guest::initialized();
    let nstuff_status = sync_guest::current_nstuff_status();
    let overclock_safe_profiles = sync_guest::overclock_uses_safe_profiles();
    let env_flags = sync_guest::env_flags();
    let overclock_profile = sync_guest::current_overclock_profile().flatten();

    let docked_profile =
        overclock_profile.and_then(|profile| profile::docked_profile_map().state_for(profile));
    let index_mode = sync_guest::index_mode().flatten();
    let render_opts_enabled = sync_guest::render_opts_enabled();

    let snapshot = RemoteSnapshot {
        remote_present,
        runtime_status,
        nstuff_status,
        overclock_safe_profiles,
        env_flags,
        overclock_profile,
        docked_profile,
        vsync_enabled: env_flags.map(|flags| flags.contains(EnvironmentFlags::VSYNC_ENABLED)),
        render_opts_enabled,
        pacer_enabled: env_flags.map(|flags| flags.contains(EnvironmentFlags::PACER_ENABLED)),
        buffer_mode: env_flags.map(|flags| {
            if flags.contains(EnvironmentFlags::TRIPLE_ENABLED) {
                BufferMode::Triple
            } else {
                BufferMode::Double
            }
        }),
        index_mode,
    };

    with_state(|state| {
        let previous_present = state.snapshot.remote_present;
        state.snapshot = snapshot;
        if previous_present != snapshot.remote_present {
            state.push_log(format!(
                "ssbusync presence changed -> {}",
                on_off(snapshot.remote_present)
            ));
        }
        if verbose {
            state.push_log(format!(
                "snapshot: remote={} runtime={} profile={} uv={} gpu_mv={} vsync={} render_opts={} pacer={} buffer={} index={}",
                on_off(snapshot.remote_present),
                option_bool(snapshot.runtime_status),
                format!(
                    "{}:{}",
                    option_overclock_preset(snapshot.overclock_safe_profiles),
                    option_overclock_profile(snapshot.overclock_profile)
                ),
                option_uv_state(snapshot.nstuff_status),
                option_uv_gpu_mv(snapshot.nstuff_status),
                option_bool(snapshot.vsync_enabled),
                option_bool(snapshot.render_opts_enabled),
                option_bool(snapshot.pacer_enabled),
                option_buffer_mode(snapshot.buffer_mode),
                option_index_mode(snapshot.index_mode)
            ));
        }
    });
}

extern "C" fn on_vsync_changed(enabled: bool) {
    with_state(|state| {
        state.snapshot.vsync_enabled = Some(enabled);
        if let Some(flags) = state.snapshot.env_flags {
            state.snapshot.env_flags = Some(flags.with(EnvironmentFlags::VSYNC_ENABLED, enabled));
        }
        state.push_log(format!("event: vsync -> {}", on_off(enabled)));
    });
}

extern "C" fn on_render_opts_changed(enabled: bool) {
    with_state(|state| {
        state.snapshot.render_opts_enabled = Some(enabled);
        if let Some(flags) = state.snapshot.env_flags {
            state.snapshot.env_flags =
                Some(flags.with(EnvironmentFlags::RENDER_OPTS_ENABLED, enabled));
        }
        state.push_log(format!("event: render_opts -> {}", on_off(enabled)));
    });
}

extern "C" fn on_buffer_mode_changed(mode: BufferMode) {
    with_state(|state| {
        state.snapshot.buffer_mode = Some(mode);
        if let Some(flags) = state.snapshot.env_flags {
            state.snapshot.env_flags = Some(flags.with(
                EnvironmentFlags::TRIPLE_ENABLED,
                matches!(mode, BufferMode::Triple),
            ));
        }
        state.push_log(format!("event: buffer -> {}", buffer_mode_name(mode)));
    });
}

extern "C" fn on_index_mode_changed(mode: IndexMode) {
    with_state(|state| {
        state.snapshot.index_mode = Some(mode);
        if let Some(flags) = state.snapshot.env_flags {
            state.snapshot.env_flags =
                Some(flags.with_value(EnvironmentFlags::INDEX_MODE, mode as u32));
        }
        state.push_log(format!("event: index mode -> {}", index_mode_name(mode)));
    });
}

fn read_command_buffer(buffer: &[u8; COMMAND_BUFFER_LEN]) -> String {
    let len = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..len]).trim().to_string()
}

fn text_line(text: &str) {
    unsafe {
        igTextUnformatted(
            text.as_ptr() as *const c_char,
            text.as_ptr().add(text.len()) as *const c_char,
        );
    }
}

const fn toggle_mask() -> u64 {
    ninput::button_mask!(
        ninput::gamepad::KEY_ZL,
        ninput::gamepad::KEY_ZR,
        ninput::gamepad::KEY_DDOWN
    )
}

const fn zero_vec2() -> ImVec2_c {
    ImVec2_c { x: 0.0, y: 0.0 }
}

fn guest_result(result: Option<bool>) -> &'static str {
    match result {
        Some(true) => "ok",
        Some(false) => "rejected",
        None => "symbol unavailable",
    }
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn option_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(value) => on_off(value),
        None => "unknown",
    }
}

fn option_buffer_mode(value: Option<BufferMode>) -> &'static str {
    value.map(buffer_mode_name).unwrap_or("unknown")
}

fn option_index_mode(value: Option<IndexMode>) -> &'static str {
    value.map(index_mode_name).unwrap_or("unknown")
}

fn option_overclock_profile(value: Option<OverclockProfile>) -> &'static str {
    value.map(overclock_profile_name).unwrap_or("unknown")
}

fn option_overclock_preset(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "safe",
        Some(false) => "default",
        None => "unknown",
    }
}

fn option_docked_profile(value: Option<profile::DockedProfile>) -> &'static str {
    value.map(docked_profile_name).unwrap_or("unknown")
}

fn option_flag(value: Option<EnvironmentFlags>, mask: u32) -> &'static str {
    match value {
        Some(flags) => on_off(flags.contains(mask)),
        None => "unknown",
    }
}

fn option_index_mode_field(value: Option<EnvironmentFlags>) -> &'static str {
    match value
        .and_then(|flags| IndexMode::from_u32(flags.extract_value(EnvironmentFlags::INDEX_MODE)))
    {
        Some(mode) => index_mode_name(mode),
        None => "unknown",
    }
}

fn option_nstuff_enabled(value: Option<sync_guest::NsTuffStatus>) -> &'static str {
    match value {
        Some(status) => on_off(status.enabled != 0),
        None => "unavailable",
    }
}

fn option_nstuff_title_match(value: Option<sync_guest::NsTuffStatus>) -> &'static str {
    match value {
        Some(status) => on_off(status.title_match != 0),
        None => "unknown",
    }
}

fn option_nstuff_poll_interval(value: Option<sync_guest::NsTuffStatus>) -> String {
    value
        .map(|status| status.poll_interval_ms.to_string())
        .unwrap_or_else(|| "unavailable".to_string())
}

fn option_nstuff_profile(value: Option<sync_guest::NsTuffStatus>) -> &'static str {
    match value.map(|status| status.profile) {
        Some(0) => "handheld",
        Some(1) => "charging-usb",
        Some(2) => "charging-official",
        Some(3) => "docked",
        Some(_) => "unknown",
        None => "unavailable",
    }
}

fn option_uv_enabled(value: Option<sync_guest::NsTuffStatus>) -> &'static str {
    match value {
        Some(status) => on_off(status.undervolt_enabled != 0),
        None => "unavailable",
    }
}

fn option_uv_state(value: Option<sync_guest::NsTuffStatus>) -> &'static str {
    match value {
        Some(status) if status.undervolt_enabled == 0 => "disabled",
        Some(status) if status.undervolt_active != 0 => "active",
        Some(_) => "restored",
        None => "unavailable",
    }
}

fn option_uv_cpu_mode(value: Option<sync_guest::NsTuffStatus>) -> String {
    value
        .map(|status| status.undervolt_cpu_mode.to_string())
        .unwrap_or_else(|| "unavailable".to_string())
}

fn option_uv_gpu_mv(value: Option<sync_guest::NsTuffStatus>) -> String {
    value
        .map(|status| status.undervolt_gpu_mv.to_string())
        .unwrap_or_else(|| "unavailable".to_string())
}

fn option_ptr<T>(value: Option<*mut T>) -> String {
    value
        .map(|ptr| format!("{ptr:p}"))
        .unwrap_or_else(|| "unavailable".to_string())
}

fn buffer_mode_name(mode: BufferMode) -> &'static str {
    match mode {
        BufferMode::Double => "double",
        BufferMode::Triple => "triple",
    }
}

fn index_mode_name(mode: IndexMode) -> &'static str {
    match mode {
        IndexMode::Immediate => "immediate",
        IndexMode::OneBehind => "onebehind",
        IndexMode::TwoBehind => "twobehind",
    }
}

fn overclock_profile_name(mode: OverclockProfile) -> &'static str {
    match mode {
        OverclockProfile::PerformanceSingles => "singles",
        OverclockProfile::PerformanceFfa => "ffa",
        OverclockProfile::Rest => "rest",
    }
}

fn docked_profile_name(mode: profile::DockedProfile) -> &'static str {
    match mode {
        profile::DockedProfile::Rest => "rest",
        profile::DockedProfile::Singles => "singles",
        profile::DockedProfile::Ffa => "ffa",
    }
}
