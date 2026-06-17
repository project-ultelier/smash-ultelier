use crate::callback::Callback;
use crate::{BufferMode, IndexMode, StateCallback};
use std::sync::Mutex;

pub type TypedVsyncCallback = extern "C" fn(bool);
pub type TypedFpsBoostCallback = extern "C" fn(bool);
pub type TypedRenderOptsCallback = extern "C" fn(bool);
pub type TypedBufferModeCallback = extern "C" fn(BufferMode);
pub type TypedIndexModeCallback = extern "C" fn(IndexMode);

static TYPED_FPS_BOOST_CHANGED: Mutex<Callback<TypedFpsBoostCallback>> =
    Mutex::new(Callback::new(None));
static TYPED_VSYNC_CHANGED: Mutex<Callback<TypedVsyncCallback>> = Mutex::new(Callback::new(None));
static TYPED_RENDER_OPTS_CHANGED: Mutex<Callback<TypedRenderOptsCallback>> =
    Mutex::new(Callback::new(None));
static TYPED_BUFFER_MODE_CHANGED: Mutex<Callback<TypedBufferModeCallback>> =
    Mutex::new(Callback::new(None));
static TYPED_INDEX_MODE_CHANGED: Mutex<Callback<TypedIndexModeCallback>> =
    Mutex::new(Callback::new(None));

fn with_typed_callback<F, R>(
    slot: &Mutex<Callback<F>>,
    f: impl FnOnce(&mut Callback<F>) -> R,
) -> R {
    let mut callback = slot.lock().unwrap_or_else(|err| err.into_inner());
    f(&mut callback)
}

extern "C" fn vsync_changed_typed_thunk(enabled: u32) {
    let enabled = enabled != 0;
    with_typed_callback(&TYPED_VSYNC_CHANGED, |callback| {
        let _ = callback.invoke((enabled,));
    });
}

extern "C" fn fps_boost_changed_typed_thunk(enabled: u32) {
    let enabled = enabled != 0;
    with_typed_callback(&TYPED_FPS_BOOST_CHANGED, |callback| {
        let _ = callback.invoke((enabled,));
    });
}

extern "C" fn render_opts_changed_typed_thunk(enabled: u32) {
    let enabled = enabled != 0;
    with_typed_callback(&TYPED_RENDER_OPTS_CHANGED, |callback| {
        let _ = callback.invoke((enabled,));
    });
}

extern "C" fn buffer_mode_changed_typed_thunk(raw: u32) {
    let Some(mode) = BufferMode::from_u32(raw) else {
        return;
    };
    with_typed_callback(&TYPED_BUFFER_MODE_CHANGED, |callback| {
        let _ = callback.invoke((mode,));
    });
}

extern "C" fn index_mode_changed_typed_thunk(raw: u32) {
    let Some(mode) = IndexMode::from_u32(raw) else {
        return;
    };
    with_typed_callback(&TYPED_INDEX_MODE_CHANGED, |callback| {
        let _ = callback.invoke((mode,));
    });
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideEffectEvent {
    VsyncChanged = 0,
    RenderOptsChanged = 1,
    BufferModeChanged = 2,
    IndexModeChanged = 3,
    FpsBoostChanged = 4,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SideEffectRegistry {
    pub fps_boost_changed: Callback<StateCallback>,
    pub vsync_changed: Callback<StateCallback>,
    pub render_opts_changed: Callback<StateCallback>,
    pub buffer_mode_changed: Callback<StateCallback>,
    pub index_mode_changed: Callback<StateCallback>,
}

impl SideEffectRegistry {
    /// Registers every populated raw callback in the remote runtime.
    ///
    /// # Example
    /// ```ignore
    /// use ultelier::sync_guest::callback::Callback;
    /// use ultelier::sync_guest::events::SideEffectRegistry;
    ///
    /// extern "C" fn on_vsync(_: u32) {}
    ///
    /// let registry = SideEffectRegistry {
    ///     vsync_changed: Callback::from_fn(on_vsync),
    ///     ..Default::default()
    /// };
    ///
    /// let ok = registry.register_remote();
    /// ```
    pub fn register_remote(&self) -> bool {
        let mut ok = true;
        if self.fps_boost_changed.is_set() {
            ok &= crate::set_fps_boost_changed_callback(self.fps_boost_changed.get()) == Some(true);
        }
        if self.vsync_changed.is_set() {
            ok &= crate::set_vsync_changed_callback(self.vsync_changed.get()) == Some(true);
        }
        if self.render_opts_changed.is_set() {
            ok &= crate::set_render_opts_changed_callback(self.render_opts_changed.get())
                == Some(true);
        }
        if self.buffer_mode_changed.is_set() {
            ok &= crate::set_buffer_mode_changed_callback(self.buffer_mode_changed.get())
                == Some(true);
        }
        if self.index_mode_changed.is_set() {
            ok &=
                crate::set_index_mode_changed_callback(self.index_mode_changed.get()) == Some(true);
        }
        ok
    }

    /// Clears all raw callbacks from the remote runtime.
    ///
    /// # Example
    /// ```ignore
    /// let ok = ultelier::sync_guest::events::SideEffectRegistry::clear_remote();
    /// ```
    pub fn clear_remote() -> bool {
        crate::clear_fps_boost_changed_callback() == Some(true)
            && crate::clear_vsync_changed_callback() == Some(true)
            && crate::clear_render_opts_changed_callback() == Some(true)
            && crate::clear_buffer_mode_changed_callback() == Some(true)
            && crate::clear_index_mode_changed_callback() == Some(true)
    }
}

/// Registers a raw FPS boost callback and returns whether registration
/// succeeded.
pub fn set_fps_boost_changed(callback: StateCallback) -> bool {
    crate::set_fps_boost_changed_callback(Some(callback)) == Some(true)
}

/// Clears the raw FPS boost callback.
pub fn clear_fps_boost_changed() -> bool {
    crate::clear_fps_boost_changed_callback() == Some(true)
}

/// Registers a raw vsync-change callback and returns whether registration
/// succeeded.
///
/// # Example
/// ```ignore
/// extern "C" fn on_vsync_changed(raw: u32) {
///     skyline::println!("vsync enabled = {}", raw != 0);
/// }
///
/// let ok = ultelier::sync_guest::events::set_vsync_changed(on_vsync_changed);
/// ```
pub fn set_vsync_changed(callback: StateCallback) -> bool {
    crate::set_vsync_changed_callback(Some(callback)) == Some(true)
}

/// Clears the raw vsync-change callback.
///
/// # Example
/// ```ignore
/// let ok = ultelier::sync_guest::events::clear_vsync_changed();
/// ```
pub fn clear_vsync_changed() -> bool {
    crate::clear_vsync_changed_callback() == Some(true)
}

/// Registers a raw render-opts change callback and returns whether registration
/// succeeded.
///
/// # Example
/// ```ignore
/// extern "C" fn on_render_opts_changed(raw: u32) {
///     skyline::println!("render-opts enabled = {}", raw != 0);
/// }
///
/// let ok = ultelier::sync_guest::events::set_render_opts_changed(on_render_opts_changed);
/// ```
pub fn set_render_opts_changed(callback: StateCallback) -> bool {
    crate::set_render_opts_changed_callback(Some(callback)) == Some(true)
}

/// Clears the raw render-opts change callback.
///
/// # Example
/// ```ignore
/// let ok = ultelier::sync_guest::events::clear_render_opts_changed();
/// ```
pub fn clear_render_opts_changed() -> bool {
    crate::clear_render_opts_changed_callback() == Some(true)
}
/// Registers a raw buffer-mode callback and returns whether registration
/// succeeded.
///
/// # Example
/// ```ignore
/// extern "C" fn on_buffer_mode_changed(raw: u32) {
///     skyline::println!("buffer mode raw = {raw}");
/// }
///
/// let ok = ultelier::sync_guest::events::set_buffer_mode_changed(on_buffer_mode_changed);
/// ```
pub fn set_buffer_mode_changed(callback: StateCallback) -> bool {
    crate::set_buffer_mode_changed_callback(Some(callback)) == Some(true)
}

/// Clears the raw buffer-mode callback.
///
/// # Example
/// ```ignore
/// let ok = ultelier::sync_guest::events::clear_buffer_mode_changed();
/// ```
pub fn clear_buffer_mode_changed() -> bool {
    crate::clear_buffer_mode_changed_callback() == Some(true)
}

/// Registers a raw index-mode callback and returns whether registration
/// succeeded.
///
/// # Example
/// ```ignore
/// extern "C" fn on_index_mode_changed(raw: u32) {
///     skyline::println!("index mode raw = {raw}");
/// }
///
/// let ok = ultelier::sync_guest::events::set_index_mode_changed(on_index_mode_changed);
/// ```
pub fn set_index_mode_changed(callback: StateCallback) -> bool {
    crate::set_index_mode_changed_callback(Some(callback)) == Some(true)
}

/// Clears the raw index-mode callback.
///
/// # Example
/// ```ignore
/// let ok = ultelier::sync_guest::events::clear_index_mode_changed();
/// ```
pub fn clear_index_mode_changed() -> bool {
    crate::clear_index_mode_changed_callback() == Some(true)
}

/// Registers a typed `bool` callback for FPS boost changes.
pub fn set_typed_fps_boost_changed(callback: TypedFpsBoostCallback) -> bool {
    with_typed_callback(&TYPED_FPS_BOOST_CHANGED, |slot| {
        let _ = slot.set(callback);
    });
    crate::set_fps_boost_changed_callback(Some(fps_boost_changed_typed_thunk)) == Some(true)
}

/// Clears the typed FPS boost callback.
pub fn clear_typed_fps_boost_changed() -> bool {
    with_typed_callback(&TYPED_FPS_BOOST_CHANGED, |slot| {
        let _ = slot.clear();
    });
    crate::clear_fps_boost_changed_callback() == Some(true)
}

/// Registers a typed `bool` callback for vsync changes.
///
/// # Example
/// ```ignore
/// extern "C" fn on_vsync_changed(enabled: bool) {
///     skyline::println!("vsync enabled = {enabled}");
/// }
///
/// let ok = ultelier::sync_guest::events::set_typed_vsync_changed(on_vsync_changed);
/// ```
pub fn set_typed_vsync_changed(callback: TypedVsyncCallback) -> bool {
    with_typed_callback(&TYPED_VSYNC_CHANGED, |slot| {
        let _ = slot.set(callback);
    });
    crate::set_vsync_changed_callback(Some(vsync_changed_typed_thunk)) == Some(true)
}

/// Clears the typed vsync callback.
///
/// # Example
/// ```ignore
/// let ok = ultelier::sync_guest::events::clear_typed_vsync_changed();
/// ```
pub fn clear_typed_vsync_changed() -> bool {
    with_typed_callback(&TYPED_VSYNC_CHANGED, |slot| {
        let _ = slot.clear();
    });
    crate::clear_vsync_changed_callback() == Some(true)
}

/// Registers a typed `bool` callback for render opts changes.
///
/// # Example
/// ```ignore
/// extern "C" fn on_render_opts_changed(enabled: bool) {
///     skyline::println!("render-opts enabled = {enabled}");
/// }
///
/// let ok = ultelier::sync_guest::events::set_typed_render_opts_changed(on_render_opts_changed);
/// ```
pub fn set_typed_render_opts_changed(callback: TypedRenderOptsCallback) -> bool {
    with_typed_callback(&TYPED_RENDER_OPTS_CHANGED, |slot| {
        let _ = slot.set(callback);
    });
    crate::set_render_opts_changed_callback(Some(render_opts_changed_typed_thunk)) == Some(true)
}

/// Clears the typed render opts callback.
///
/// # Example
/// ```ignore
/// let ok = ultelier::sync_guest::events::clear_typed_render_opts_changed();
/// ```
pub fn clear_typed_render_opts_changed() -> bool {
    with_typed_callback(&TYPED_RENDER_OPTS_CHANGED, |slot| {
        let _ = slot.clear();
    });
    crate::clear_render_opts_changed_callback() == Some(true)
}

/// Registers a typed `BufferMode` callback for buffer-mode changes.
///
/// # Example
/// ```ignore
/// use ultelier::sync_guest::BufferMode;
///
/// extern "C" fn on_buffer_mode_changed(mode: BufferMode) {
///     skyline::println!("buffer mode = {:?}", mode);
/// }
///
/// let ok = ultelier::sync_guest::events::set_typed_buffer_mode_changed(on_buffer_mode_changed);
/// ```
pub fn set_typed_buffer_mode_changed(callback: TypedBufferModeCallback) -> bool {
    with_typed_callback(&TYPED_BUFFER_MODE_CHANGED, |slot| {
        let _ = slot.set(callback);
    });
    crate::set_buffer_mode_changed_callback(Some(buffer_mode_changed_typed_thunk)) == Some(true)
}

/// Clears the typed buffer-mode callback.
///
/// # Example
/// ```ignore
/// let ok = ultelier::sync_guest::events::clear_typed_buffer_mode_changed();
/// ```
pub fn clear_typed_buffer_mode_changed() -> bool {
    with_typed_callback(&TYPED_BUFFER_MODE_CHANGED, |slot| {
        let _ = slot.clear();
    });
    crate::clear_buffer_mode_changed_callback() == Some(true)
}

/// Registers a typed `IndexMode` callback for index-mode changes.
///
/// # Example
/// ```ignore
/// use ultelier::sync_guest::IndexMode;
///
/// extern "C" fn on_index_mode_changed(mode: IndexMode) {
///     skyline::println!("index mode = {:?}", mode);
/// }
///
/// let ok = ultelier::sync_guest::events::set_typed_index_mode_changed(on_index_mode_changed);
/// ```
pub fn set_typed_index_mode_changed(callback: TypedIndexModeCallback) -> bool {
    with_typed_callback(&TYPED_INDEX_MODE_CHANGED, |slot| {
        let _ = slot.set(callback);
    });
    crate::set_index_mode_changed_callback(Some(index_mode_changed_typed_thunk)) == Some(true)
}

/// Clears the typed index-mode callback.
///
/// # Example
/// ```ignore
/// let ok = ultelier::sync_guest::events::clear_typed_index_mode_changed();
/// ```
pub fn clear_typed_index_mode_changed() -> bool {
    with_typed_callback(&TYPED_INDEX_MODE_CHANGED, |slot| {
        let _ = slot.clear();
    });
    crate::clear_index_mode_changed_callback() == Some(true)
}
