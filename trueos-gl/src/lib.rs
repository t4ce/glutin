//! In-process GL state and UI4 presentation ownership.
//!
//! Procedure pointers are local code, not host function pointers crossing a VM
//! boundary. Only the versioned resource/submission C ABI crosses that
//! boundary.
use std::cell::RefCell;
use std::ffi::{CStr, c_void};
use std::sync::{Arc, Mutex, Weak};
use std::thread::ThreadId;

#[doc(hidden)]
pub mod abi;
use abi as vcabi;
#[path = "gl_objects.rs"]
mod objects;

const BAD_CONTEXT: i32 = -9;
const BAD_SURFACE: i32 = -5;
const UNSUPPORTED: i32 = -95;
const BUSY: i32 = -16;
const LOST: i32 = -32;
const INVALID_ENUM: u32 = 0x500;
const INVALID_VALUE: u32 = 0x501;
const INVALID_OPERATION: u32 = 0x502;

fn rc(value: i32) -> Result<(), i32> {
    if value == 0 { Ok(()) } else { Err(value) }
}

#[derive(Debug)]
struct Device {
    device: u64,
    queue: u64,
}
impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            vcabi::trueos_cabi_vgpu_queue_destroy(self.device, self.queue);
            vcabi::trueos_cabi_vgpu_close(self.device);
        }
    }
}

#[derive(Debug)]
struct Lease {
    device: Arc<Device>,
    info: vcabi::SurfaceInfo,
}
impl Lease {
    fn discard(mut self) -> Result<(), i32> {
        let surface = std::mem::take(&mut self.info.surface);
        rc(unsafe { vcabi::trueos_cabi_vgpu_ui4_surface_discard(self.device.device, surface) })
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if self.info.surface != 0 {
            unsafe {
                vcabi::trueos_cabi_vgpu_ui4_surface_discard(self.device.device, self.info.surface);
            }
        }
    }
}

#[derive(Debug)]
struct Frame {
    window: u32,
    width: u32,
    height: u32,
    owner: Option<u64>,
    lease: Option<Lease>,
    failure: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct Surface(Arc<Mutex<Frame>>);
impl Surface {
    pub fn new(window: u32, width: u32, height: u32) -> Self {
        Self(Arc::new(Mutex::new(Frame {
            window,
            width,
            height,
            owner: None,
            lease: None,
            failure: None,
        })))
    }

    pub fn size(&self) -> (u32, u32) {
        let frame = self.0.lock().unwrap();
        (frame.width, frame.height)
    }

    fn acquire(frame: &mut Frame, device: &Arc<Device>) -> Result<(), i32> {
        let mut info = vcabi::SurfaceInfo::default();
        rc(unsafe {
            vcabi::trueos_cabi_vgpu_ui4_surface_acquire(device.device, frame.window, &mut info)
        })?;
        let lease = Lease { device: device.clone(), info };
        if info.surface == 0
            || info.width == 0
            || info.height == 0
            || info.width > i32::MAX as u32
            || info.height > i32::MAX as u32
            || u64::from(info.pitch) < u64::from(info.width) * 4
            || info.bytes < u64::from(info.pitch) * u64::from(info.height)
            || info.format != vcabi::SURFACE_FORMAT_RGBA8_UNORM_SRGB
        {
            return Err(BAD_SURFACE);
        }
        frame.width = info.width;
        frame.height = info.height;
        frame.lease = Some(lease);
        Ok(())
    }
}

#[derive(Debug)]
struct State {
    device: Arc<Device>,
    thread: Option<ThreadId>,
    error: u32,
    failure: Option<i32>,
    clear_color: [f32; 4],
    clear: Option<u32>,
    viewport: [i32; 4],
    // Never present a partial frame after an unsupported draw.
    pending_draw: bool,
    objects: objects::Objects,
}
impl State {
    fn error(&mut self, error: u32) {
        if self.error == 0 {
            self.error = error;
        }
    }
}

#[derive(Clone)]
struct Binding {
    state: Weak<Mutex<State>>,
    surface: Surface,
}
thread_local! { static CURRENT: RefCell<Option<Binding>> = const { RefCell::new(None) }; }

#[derive(Debug)]
pub struct Context(Arc<Mutex<State>>, std::marker::PhantomData<std::cell::Cell<()>>);
impl Context {
    pub fn new() -> Result<Self, i32> {
        let mut device = 0;
        // Render queue, timeline, drawing and presentation.
        rc(unsafe {
            vcabi::trueos_cabi_vgpu_open((1 << 1) | (1 << 2) | (1 << 4) | (1 << 6), &mut device)
        })?;
        if device == 0 {
            return Err(BAD_CONTEXT);
        }
        let mut queue = 0;
        let result = rc(unsafe { vcabi::trueos_cabi_vgpu_queue_create(device, 1, &mut queue) });
        if result.is_err() || queue == 0 {
            unsafe {
                vcabi::trueos_cabi_vgpu_close(device);
            }
            return Err(result.err().unwrap_or(BAD_CONTEXT));
        }
        Ok(Self(
            Arc::new(Mutex::new(State {
                device: Arc::new(Device { device, queue }),
                thread: None,
                error: 0,
                failure: None,
                clear_color: [0.; 4],
                clear: None,
                viewport: [0; 4],
                pending_draw: false,
                objects: objects::Objects::default(),
            })),
            std::marker::PhantomData,
        ))
    }

    pub fn id(&self) -> u64 {
        self.0.lock().unwrap().device.device
    }

    pub fn is_current(&self) -> bool {
        CURRENT.with(|current| {
            current
                .borrow()
                .as_ref()
                .and_then(|b| b.state.upgrade())
                .is_some_and(|state| Arc::ptr_eq(&state, &self.0))
        })
    }

    pub fn is_surface_current(&self, surface: &Surface) -> bool {
        self.is_current()
            && CURRENT.with(|current| {
                current
                    .borrow()
                    .as_ref()
                    .is_some_and(|binding| Arc::ptr_eq(&binding.surface.0, &surface.0))
            })
    }

    pub fn make_current(&self, surface: &Surface) -> Result<(), i32> {
        if self.is_surface_current(surface) {
            return self.0.lock().unwrap().failure.map_or(Ok(()), Err);
        }
        ensure_current_frame_idle()?;
        let mut state = self.0.lock().unwrap();
        if let Some(error) = state.failure {
            return Err(error);
        }
        if state.thread.is_some_and(|thread| thread != std::thread::current().id()) {
            return Err(BUSY);
        }
        let mut frame = surface.0.lock().unwrap();
        if frame.owner.is_some() {
            return Err(BUSY);
        }
        if let Some(error) = frame.failure {
            return Err(error);
        }
        Surface::acquire(&mut frame, &state.device)?;
        frame.owner = Some(state.device.device);
        if state.viewport == [0; 4] {
            state.viewport = [0, 0, frame.width as i32, frame.height as i32];
        }
        drop(frame);
        drop(state);
        // Commit the switch only once acquiring the destination has succeeded.
        let switched = CURRENT.with(|current| {
            if let Some(old) = current.borrow_mut().take() {
                release(old)?;
            }
            *current.borrow_mut() =
                Some(Binding { state: Arc::downgrade(&self.0), surface: surface.clone() });
            Ok(())
        });
        if let Err(error) = switched {
            let mut frame = surface.0.lock().unwrap();
            frame.owner = None;
            if let Some(lease) = frame.lease.take() {
                if let Err(discard_error) = lease.discard() {
                    frame.failure = Some(discard_error);
                }
            }
            return Err(error);
        }
        self.0.lock().unwrap().thread = Some(std::thread::current().id());
        Ok(())
    }

    pub fn make_not_current(&self) -> Result<(), i32> {
        if self.is_current() {
            ensure_current_frame_idle()?;
            return CURRENT.with(|current| current.borrow_mut().take().map_or(Ok(()), release));
        } else if self.0.lock().unwrap().thread.is_some() {
            return Err(BUSY);
        }
        Ok(())
    }

    pub fn swap(&self, surface: &Surface) -> Result<(), i32> {
        if !self.is_surface_current(surface) {
            return Err(BAD_CONTEXT);
        }
        let mut state = self.0.lock().unwrap();
        if let Some(error) = state.failure {
            return Err(error);
        }
        let mut frame = surface.0.lock().unwrap();
        if let Some(error) = frame.failure {
            return Err(error);
        }
        // AOT draws require the native Bakery packages and a matching submission
        // implementation. Do not turn them into a successful clear-only frame.
        if state.pending_draw {
            return Err(UNSUPPORTED);
        }
        let color = state.clear.ok_or(UNSUPPORTED)?;
        let mut lease = frame.lease.take().ok_or(BAD_SURFACE)?;
        let mut point = vcabi::TimelinePoint::default();
        let handle = std::mem::take(&mut lease.info.surface);
        let result = rc(unsafe {
            vcabi::trueos_cabi_vgpu_ui4_surface_clear_submit(
                state.device.device,
                state.device.queue,
                handle,
                color,
                &mut point,
            )
        });
        // The host owns retirement after submit, including ambiguous failures.
        // Do not recycle the submitted lease after an error.
        let result = result.and_then(|()| {
            if point.queue != state.device.queue || point.value == 0 {
                return Err(LOST);
            }
            rc(unsafe {
                vcabi::trueos_cabi_vgpu_wait(state.device.device, point.queue, point.value)
            })
        });
        if let Err(error) = result {
            state.failure = Some(error);
            return Err(error);
        }
        state.clear = None;
        if let Err(error) = Surface::acquire(&mut frame, &state.device) {
            frame.failure = Some(error);
            return Err(error);
        }
        Ok(())
    }

    pub fn resize(&self, surface: &Surface, width: u32, height: u32) -> Result<(), i32> {
        if !self.is_surface_current(surface) {
            return Err(BAD_CONTEXT);
        }
        ensure_current_frame_idle()?;
        let state = self.0.lock().unwrap();
        let mut frame = surface.0.lock().unwrap();
        if let Some(error) = state.failure.or(frame.failure) {
            return Err(error);
        }
        if let Some(lease) = frame.lease.take() {
            if let Err(error) = lease.discard() {
                frame.failure = Some(error);
                return Err(error);
            }
        }
        let result =
            rc(unsafe { vcabi::trueos_cabi_ui4_scene_frame_resize(frame.window, width, height) })
                .and_then(|()| Surface::acquire(&mut frame, &state.device));
        if let Err(error) = result {
            frame.failure = Some(error);
        }
        result
    }

    pub fn record_failure(&self, error: i32) {
        self.0.lock().unwrap().failure = Some(error);
    }
}
impl Drop for Context {
    fn drop(&mut self) {
        if self.is_current() {
            CURRENT.with(|current| {
                if let Some(binding) = current.borrow_mut().take() {
                    let _ = release(binding);
                }
            });
        }
    }
}
fn ensure_current_frame_idle() -> Result<(), i32> {
    with_state(Ok(()), |state| {
        // Commands belong to the drawable on which they were issued. Until
        // native replay can flush them, never move them to a different window.
        if state.clear.is_some() || state.pending_draw { Err(UNSUPPORTED) } else { Ok(()) }
    })
}
fn release(binding: Binding) -> Result<(), i32> {
    let state = binding.state.upgrade();
    if let Some(state) = &state {
        state.lock().unwrap().thread = None;
    }
    let mut frame = binding.surface.0.lock().unwrap();
    frame.owner = None;
    let result = frame.lease.take().map_or(Ok(()), Lease::discard);
    if let Err(error) = result {
        frame.failure = Some(error);
    }
    drop(frame);
    if let (Some(state), Err(error)) = (state, result) {
        state.lock().unwrap().failure = Some(error);
    }
    result
}

fn with_state<R>(default: R, callback: impl FnOnce(&mut State) -> R) -> R {
    let state = CURRENT.with(|current| current.borrow().as_ref().and_then(|b| b.state.upgrade()));
    match state {
        Some(state) => callback(&mut state.lock().unwrap()),
        None => default,
    }
}

unsafe extern "system" fn get_error() -> u32 {
    with_state(INVALID_OPERATION, |s| std::mem::take(&mut s.error))
}
unsafe extern "system" fn get_string(name: u32) -> *const u8 {
    with_state(std::ptr::null(), |s| match name {
        0x1F00 => c"TRUEOS".as_ptr().cast(),
        0x1F01 => c"TRUEOS Intel AOT".as_ptr().cast(),
        0x1F02 => c"OpenGL ES 2.0 TRUEOS".as_ptr().cast(),
        0x8B8C => c"OpenGL ES GLSL ES 1.00".as_ptr().cast(),
        0x1F03 => c"".as_ptr().cast(),
        _ => {
            s.error(INVALID_ENUM);
            std::ptr::null()
        },
    })
}
unsafe extern "system" fn clear_color(r: f32, g: f32, b: f32, a: f32) {
    with_state((), |s| {
        s.clear_color = [r, g, b, a].map(|v| if v.is_nan() { 0. } else { v.clamp(0., 1.) })
    });
}
unsafe extern "system" fn clear(mask: u32) {
    with_state((), |s| {
        if mask == 0 {
            return;
        }
        if mask != 0x4000 {
            s.error(INVALID_VALUE);
            return;
        }
        s.clear = Some(u32::from_le_bytes(s.clear_color.map(|v| (v * 255.).round() as u8)));
        s.pending_draw = false;
    });
}
unsafe extern "system" fn viewport(x: i32, y: i32, width: i32, height: i32) {
    with_state((), |s| {
        if width < 0 || height < 0 {
            s.error(INVALID_VALUE)
        } else {
            s.viewport = [x, y, width, height]
        }
    });
}
unsafe extern "system" fn finish() {
    with_state((), |s| {
        if s.pending_draw || s.clear.is_some() {
            s.error(INVALID_OPERATION);
        }
    });
}

pub fn resolve(name: &CStr) -> *const c_void {
    match name.to_bytes() {
        b"glGetError" => get_error as *const c_void,
        b"glGetString" => get_string as *const c_void,
        b"glClearColor" => clear_color as *const c_void,
        b"glClear" => clear as *const c_void,
        b"glViewport" => viewport as *const c_void,
        b"glFinish" | b"glFlush" => finish as *const c_void,
        _ => objects::resolve(name),
    }
}

/// Import a ready native font sprite into the current context's texture namespace.
/// Pixels remain kernel-owned. This does not imply that native text draws are executable yet.
pub fn import_font_sprite(
    window: u32,
    ticket: u64,
    sprite: u32,
    width: u32,
    height: u32,
) -> Result<u32, u32> {
    let current_window = CURRENT
        .with(|current| current.borrow().as_ref().map(|b| b.surface.0.lock().unwrap().window));
    if current_window != Some(window)
        || window == 0
        || ticket == 0
        || sprite == 0
        || width == 0
        || height == 0
        || width > 4096
        || height > 4096
    {
        return Err(INVALID_OPERATION);
    }
    let mut status = vcabi::FontSpriteStatus::default();
    let rc =
        unsafe { vcabi::trueos_cabi_ui4_scene_font_sprite_status_v1(window, ticket, &mut status) };
    if rc != 0
        || status.state != 2
        || (status.sprite, status.width, status.height) != (sprite, width, height)
    {
        return Err(INVALID_OPERATION);
    }
    objects::import_font(window, ticket, sprite, width, height)
}
