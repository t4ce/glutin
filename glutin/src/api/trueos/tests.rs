//! Host ABI double: runs the real backend and local GL procedure pointers.
use super::*;
use crate::config::ConfigTemplateBuilder;
use crate::context::{ContextApi, ContextAttributesBuilder, Version};
use crate::prelude::*;
use crate::surface::{SurfaceAttributesBuilder, WindowSurface};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, TrueosDisplayHandle, TrueosWindowHandle,
};
use std::collections::{BTreeMap, BTreeSet};
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());
static HOST: Mutex<Host> = Mutex::new(Host::new());
struct Host {
    next: u64,
    devices: BTreeSet<u64>,
    queues: BTreeSet<u64>,
    leases: BTreeMap<u64, u64>,
    events: Vec<String>,
    refs: i32,
    size: (u32, u32),
    fail_acquire: bool,
    fail_submit: bool,
    fail_discard: bool,
}
impl Host {
    const fn new() -> Self {
        Self {
            next: 1,
            devices: BTreeSet::new(),
            queues: BTreeSet::new(),
            leases: BTreeMap::new(),
            events: Vec::new(),
            refs: 0,
            size: (80, 60),
            fail_acquire: false,
            fail_submit: false,
            fail_discard: false,
        }
    }
}
fn fixture() -> std::sync::MutexGuard<'static, ()> {
    let guard = SERIAL.lock().unwrap();
    *HOST.lock().unwrap() = Host::new();
    guard
}
#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_ui4_display_retain_v1(connection: u64) -> i32 {
    if connection != 7 {
        return -9;
    }
    HOST.lock().unwrap().refs += 1;
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_ui4_display_close_v1(_: u64) -> i32 {
    HOST.lock().unwrap().refs -= 1;
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_ui4_display_validate_window_v1(c: u64, w: u32) -> i32 {
    if c == 7 && (1..=2).contains(&w) { 0 } else { -9 }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_vgpu_open(caps: u64, out: *mut u64) -> i32 {
    assert_eq!(caps, (1 << 1) | (1 << 2) | (1 << 4) | (1 << 6));
    let mut h = HOST.lock().unwrap();
    let id = h.next;
    h.next += 1;
    h.devices.insert(id);
    unsafe { out.write(id) };
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_vgpu_close(device: u64) -> i32 {
    let mut h = HOST.lock().unwrap();
    assert!(h.devices.remove(&device));
    h.leases.retain(|_, d| *d != device);
    h.events.push("close".into());
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_vgpu_queue_create(_: u64, class: u32, out: *mut u64) -> i32 {
    assert_eq!(class, 1);
    let mut h = HOST.lock().unwrap();
    let id = h.next;
    h.next += 1;
    h.queues.insert(id);
    unsafe { out.write(id) };
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_vgpu_queue_destroy(_: u64, queue: u64) -> i32 {
    assert!(HOST.lock().unwrap().queues.remove(&queue));
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_vgpu_ui4_surface_acquire(
    device: u64,
    _: u32,
    out: *mut vcabi::SurfaceInfo,
) -> i32 {
    let mut h = HOST.lock().unwrap();
    if h.fail_acquire {
        return -16;
    }
    let id = h.next;
    h.next += 1;
    h.leases.insert(id, device);
    h.events.push(format!("acquire:{id}"));
    let (width, height) = h.size;
    unsafe {
        out.write(vcabi::SurfaceInfo {
            surface: id,
            bytes: u64::from(width) * u64::from(height) * 4,
            width,
            height,
            pitch: width * 4,
            format: 1,
        })
    };
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_vgpu_ui4_surface_discard(device: u64, surface: u64) -> i32 {
    let mut h = HOST.lock().unwrap();
    if h.fail_discard {
        return -32;
    }
    assert_eq!(h.leases.remove(&surface), Some(device));
    h.events.push(format!("discard:{surface}"));
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_vgpu_ui4_surface_clear_submit(
    device: u64,
    queue: u64,
    surface: u64,
    color: u32,
    out: *mut vcabi::TimelinePoint,
) -> i32 {
    let mut h = HOST.lock().unwrap();
    assert_eq!(h.leases.get(&surface), Some(&device));
    h.events.push(format!("submit:{color:08x}"));
    if h.fail_submit {
        return -32;
    }
    h.leases.remove(&surface);
    unsafe {
        out.write(vcabi::TimelinePoint {
            queue,
            value: 1,
            physical_serial: 1,
            physical_publish_sequence: 1,
        })
    };
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_vgpu_wait(_: u64, _: u64, _: u64) -> i32 {
    HOST.lock().unwrap().events.push("wait".into());
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_ui4_scene_frame_resize(_: u32, width: u32, height: u32) -> i32 {
    let mut h = HOST.lock().unwrap();
    h.size = (width, height);
    h.events.push("resize".into());
    0
}

fn display() -> display::Display {
    unsafe {
        display::Display::new(RawDisplayHandle::Trueos(TrueosDisplayHandle::new(
            NonZeroU64::new(7).unwrap(),
        )))
        .unwrap()
    }
}
fn setup(
    d: &display::Display,
    window: u32,
) -> (context::NotCurrentContext, surface::Surface<WindowSurface>) {
    unsafe {
        let config = d
            .find_configs(
                ConfigTemplateBuilder::new()
                    .with_depth_size(0)
                    .with_stencil_size(0)
                    .with_alpha_size(8)
                    .build(),
            )
            .unwrap()
            .next()
            .unwrap();
        let attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(Version::new(2, 0))))
            .build(None);
        let context = d.create_context(&config, &attrs).unwrap();
        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().with_srgb(Some(false)).build(
            RawWindowHandle::Trueos(TrueosWindowHandle::new(NonZeroU32::new(window).unwrap())),
            NonZeroU32::new(80).unwrap(),
            NonZeroU32::new(60).unwrap(),
        );
        let surface = d.create_window_surface(&config, &attrs).unwrap();
        (context, surface)
    }
}
unsafe fn clear(d: &display::Display) {
    unsafe {
        let color: unsafe extern "system" fn(f32, f32, f32, f32) =
            std::mem::transmute(d.get_proc_address(c"glClearColor"));
        let clear: unsafe extern "system" fn(u32) =
            std::mem::transmute(d.get_proc_address(c"glClear"));
        color(1., 0.5, 0., 1.);
        clear(0x4000);
    }
}
#[test]
fn public_api_clear_submits_waits_and_reacquires() {
    let _guard = fixture();
    {
        let d = display();
        let (c, s) = setup(&d, 1);
        let c = c.make_current(&s).unwrap();
        unsafe { clear(&d) };
        s.swap_buffers(&c).unwrap();
        assert!(s.is_current(&c));
        assert_eq!(HOST.lock().unwrap().leases.len(), 1);
        let events = HOST.lock().unwrap().events.clone();
        assert_eq!(&events[1..3], &["submit:ff0080ff", "wait"]);
        assert!(events[3].starts_with("acquire:"));
        unsafe { clear(&d) };
        s.swap_buffers(&c).unwrap();
    }
    let h = HOST.lock().unwrap();
    assert!(h.leases.is_empty() && h.devices.is_empty() && h.queues.is_empty());
    assert_eq!(h.refs, 0);
}
#[test]
fn repeated_binding_unbind_and_rebind_do_not_reuse_consumed_leases() {
    let _guard = fixture();
    {
        let d = display();
        let (c, s) = setup(&d, 1);
        let c = c.make_current(&s).unwrap();
        c.make_current(&s).unwrap();
        assert_eq!(HOST.lock().unwrap().events.len(), 1);
        c.make_not_current_in_place().unwrap();
        assert!(!s.is_current(&c));
        assert!(HOST.lock().unwrap().leases.is_empty());
        c.make_current(&s).unwrap();
        assert_eq!(HOST.lock().unwrap().events.len(), 3);
    }
}
#[test]
fn failed_switch_preserves_old_current_binding() {
    let _guard = fixture();
    {
        let d = display();
        let (c, s) = setup(&d, 1);
        let (other, other_surface) = setup(&d, 2);
        let c = c.make_current(&s).unwrap();
        let other = other.treat_as_possibly_current();
        HOST.lock().unwrap().fail_acquire = true;
        assert!(other.make_current(&other_surface).is_err());
        assert!(s.is_current(&c));
        assert!(!other.is_current());
        HOST.lock().unwrap().fail_acquire = false;
        other.make_current(&other_surface).unwrap();
        assert!(!c.is_current());
        assert!(other_surface.is_current(&other));
    }
}
#[test]
fn resize_discards_then_reacquires_and_updates_dimensions() {
    let _guard = fixture();
    {
        let d = display();
        let (c, s) = setup(&d, 1);
        let c = c.make_current(&s).unwrap();
        s.resize(&c, NonZeroU32::new(100).unwrap(), NonZeroU32::new(70).unwrap());
        assert_eq!((s.width(), s.height()), (Some(100), Some(70)));
        let events = HOST.lock().unwrap().events.clone();
        assert!(events[1].starts_with("discard:"));
        assert_eq!(events[2], "resize");
        assert!(events[3].starts_with("acquire:"));
    }
}
#[test]
fn failed_submit_is_sticky_and_does_not_recycle_an_inflight_lease() {
    let _guard = fixture();
    {
        let d = display();
        let (c, s) = setup(&d, 1);
        let c = c.make_current(&s).unwrap();
        unsafe { clear(&d) };
        HOST.lock().unwrap().fail_submit = true;
        assert!(s.swap_buffers(&c).is_err());
        assert!(s.swap_buffers(&c).is_err());
        let events = HOST.lock().unwrap().events.clone();
        assert_eq!(events.len(), 2);
        assert!(HOST.lock().unwrap().leases.len() == 1);
    }
    assert!(HOST.lock().unwrap().leases.is_empty());
}
#[test]
fn wrong_context_and_empty_frames_do_not_submit() {
    let _guard = fixture();
    {
        let d = display();
        let (c, s) = setup(&d, 1);
        let (other, other_surface) = setup(&d, 2);
        let c = c.make_current(&s).unwrap();
        let other = other.treat_as_possibly_current();
        assert!(s.swap_buffers(&other).is_err());
        assert!(other_surface.swap_buffers(&c).is_err());
        assert!(s.swap_buffers(&c).is_err());
        assert_eq!(HOST.lock().unwrap().events.len(), 1);
    }
}
#[test]
fn display_is_retained_once_and_invalid_capabilities_are_rejected() {
    let _guard = fixture();
    {
        let d = display();
        let clone = d.clone();
        drop(d);
        assert_eq!(HOST.lock().unwrap().refs, 1);
        drop(clone);
        assert_eq!(HOST.lock().unwrap().refs, 0);
        assert!(
            unsafe {
                display::Display::new(RawDisplayHandle::Trueos(TrueosDisplayHandle::new(
                    NonZeroU64::new(8).unwrap(),
                )))
            }
            .is_err()
        );
    }
}

macro_rules! proc {
    ($display:expr, $name:literal, $ty:ty) => {{
        let name = std::ffi::CStr::from_bytes_with_nul(concat!($name, "\0").as_bytes()).unwrap();
        let pointer = $display.get_proc_address(name);
        assert!(!pointer.is_null(), "missing {}", $name);
        unsafe { std::mem::transmute::<*const std::ffi::c_void, $ty>(pointer) }
    }};
}
#[test]
fn alacritty_gles2_entry_points_are_local_and_extensions_stay_unadvertised() {
    let _guard = fixture();
    {
        let d = display();
        let (c, s) = setup(&d, 1);
        let _c = c.make_current(&s).unwrap();
        for name in [
            "GetError",
            "GetString",
            "GetIntegerv",
            "Clear",
            "ClearColor",
            "Viewport",
            "Finish",
            "GenBuffers",
            "BindBuffer",
            "BufferData",
            "BufferSubData",
            "DeleteBuffers",
            "GenVertexArrays",
            "BindVertexArray",
            "DeleteVertexArrays",
            "EnableVertexAttribArray",
            "VertexAttribPointer",
            "GenTextures",
            "BindTexture",
            "TexImage2D",
            "TexSubImage2D",
            "TexParameteri",
            "PixelStorei",
            "ActiveTexture",
            "DeleteTextures",
            "UseProgram",
            "DeleteProgram",
            "Uniform1f",
            "Uniform1i",
            "Uniform4f",
            "BlendFunc",
            "BlendFuncSeparate",
            "Enable",
            "DepthMask",
            "DrawArrays",
            "DrawElements",
        ] {
            let name = std::ffi::CString::new(format!("gl{name}")).unwrap();
            assert!(!d.get_proc_address(&name).is_null(), "{name:?}");
        }
        let string = proc!(d, "glGetString", unsafe extern "system" fn(u32) -> *const u8);
        assert_eq!(unsafe { std::ffi::CStr::from_ptr(string(0x1F03).cast()) }.to_bytes(), b"");
        assert!(d.get_proc_address(c"glMadeUpExtension").is_null());
    }
}
#[test]
fn buffer_upload_bounds_and_errors_are_context_local() {
    let _guard = fixture();
    {
        let d = display();
        let (c, s) = setup(&d, 1);
        let (other, other_surface) = setup(&d, 2);
        let c = c.make_current(&s).unwrap();
        let generate = proc!(d, "glGenBuffers", unsafe extern "system" fn(i32, *mut u32));
        let bind = proc!(d, "glBindBuffer", unsafe extern "system" fn(u32, u32));
        let data = proc!(
            d,
            "glBufferData",
            unsafe extern "system" fn(u32, isize, *const std::ffi::c_void, u32)
        );
        let sub = proc!(
            d,
            "glBufferSubData",
            unsafe extern "system" fn(u32, isize, isize, *const std::ffi::c_void)
        );
        let error = proc!(d, "glGetError", unsafe extern "system" fn() -> u32);
        let mut buffer = 0;
        unsafe {
            generate(1, &mut buffer);
            bind(0x8892, buffer);
            data(0x8892, 4, std::ptr::null(), 0x88E0);
            let bytes = [1u8; 4];
            sub(0x8892, 1, 4, bytes.as_ptr().cast());
        }
        let other = other.make_current(&other_surface).unwrap();
        assert_eq!(unsafe { error() }, 0);
        c.make_current(&s).unwrap();
        assert_eq!(unsafe { error() }, 0x501);
        assert_eq!(unsafe { error() }, 0);
        drop(other);
    }
}
#[test]
fn valid_rectangle_draw_cannot_silently_present_only_its_clear() {
    let _guard = fixture();
    {
        let d = display();
        let (c, s) = setup(&d, 1);
        let c = c.make_current(&s).unwrap();
        unsafe { clear(&d) };
        let generate = proc!(d, "glGenBuffers", unsafe extern "system" fn(i32, *mut u32));
        let bind = proc!(d, "glBindBuffer", unsafe extern "system" fn(u32, u32));
        let data = proc!(
            d,
            "glBufferData",
            unsafe extern "system" fn(u32, isize, *const std::ffi::c_void, u32)
        );
        let attr = proc!(
            d,
            "glVertexAttribPointer",
            unsafe extern "system" fn(u32, i32, u32, u8, i32, *const std::ffi::c_void)
        );
        let enable = proc!(d, "glEnableVertexAttribArray", unsafe extern "system" fn(u32));
        let program = proc!(d, "glUseProgram", unsafe extern "system" fn(u32));
        let draw = proc!(d, "glDrawArrays", unsafe extern "system" fn(u32, i32, i32));
        let error = proc!(d, "glGetError", unsafe extern "system" fn() -> u32);
        let mut buffer = 0;
        unsafe {
            generate(1, &mut buffer);
            bind(0x8892, buffer);
            data(0x8892, 36, std::ptr::null(), 0x88E0);
            attr(0, 2, 0x1406, 0, 12, std::ptr::null());
            enable(0);
            attr(1, 4, 0x1401, 1, 12, 8usize as *const _);
            enable(1);
            program(0x54520002);
            draw(4, 0, 3);
        }
        assert_eq!(unsafe { error() }, 0);
        assert!(s.swap_buffers(&c).is_err());
        assert_eq!(HOST.lock().unwrap().events.len(), 1);
        // A later full clear supersedes the unsupported draw and can be submitted.
        unsafe { clear(&d) };
        s.swap_buffers(&c).unwrap();
    }
}
#[test]
fn atlas_uploads_validate_storage_and_row_bounds() {
    let _guard = fixture();
    {
        let d = display();
        let (c, s) = setup(&d, 1);
        let _c = c.make_current(&s).unwrap();
        let generate = proc!(d, "glGenTextures", unsafe extern "system" fn(i32, *mut u32));
        let bind = proc!(d, "glBindTexture", unsafe extern "system" fn(u32, u32));
        let image = proc!(
            d,
            "glTexImage2D",
            unsafe extern "system" fn(
                u32,
                i32,
                i32,
                i32,
                i32,
                i32,
                u32,
                u32,
                *const std::ffi::c_void,
            )
        );
        let sub = proc!(
            d,
            "glTexSubImage2D",
            unsafe extern "system" fn(
                u32,
                i32,
                i32,
                i32,
                i32,
                i32,
                u32,
                u32,
                *const std::ffi::c_void,
            )
        );
        let error = proc!(d, "glGetError", unsafe extern "system" fn() -> u32);
        let mut texture = 0;
        unsafe {
            generate(1, &mut texture);
            bind(0xDE1, texture);
            image(0xDE1, 0, 0x1908, 2, 2, 0, 0x1908, 0x1401, std::ptr::null());
            let pixels = [255u8; 16];
            sub(0xDE1, 0, 0, 0, 2, 2, 0x1908, 0x1401, pixels.as_ptr().cast());
            assert_eq!(error(), 0);
            sub(0xDE1, 0, 1, 0, 2, 2, 0x1908, 0x1401, pixels.as_ptr().cast());
            assert_eq!(error(), 0x501);
        }
    }
}

#[test]
fn pending_commands_cannot_follow_a_context_to_another_window() {
    let _guard = fixture();
    let d = display();
    let (c, s) = setup(&d, 1);
    let (other, other_surface) = setup(&d, 2);
    let c = c.make_current(&s).unwrap();
    let other = other.treat_as_possibly_current();
    unsafe { clear(&d) };
    assert!(other.make_current(&other_surface).is_err());
    assert!(c.make_not_current_in_place().is_err());
    assert!(s.is_current(&c));
    assert_eq!(HOST.lock().unwrap().events.len(), 1);
    s.swap_buffers(&c).unwrap();
    other.make_current(&other_surface).unwrap();
    assert!(!c.is_current());
}

#[test]
fn fixed_config_and_context_reject_unimplemented_requests() {
    let _guard = fixture();
    let d = display();
    unsafe {
        let template = ConfigTemplateBuilder::new().with_depth_size(0).with_stencil_size(0);
        assert!(
            d.find_configs(template.clone().with_alpha_size(10).build()).unwrap().next().is_none()
        );
        let config = d.find_configs(template.build()).unwrap().next().unwrap();
        assert!(!config.srgb_capable());
        for api in [ContextApi::OpenGl(None), ContextApi::Gles(Some(Version::new(3, 0)))] {
            let attrs = ContextAttributesBuilder::new().with_context_api(api).build(None);
            assert!(d.create_context(&config, &attrs).is_err());
        }
    }
    assert!(HOST.lock().unwrap().devices.is_empty());
}

#[test]
fn failed_unbind_invalidates_the_lease_and_context() {
    let _guard = fixture();
    {
        let d = display();
        let (c, s) = setup(&d, 1);
        let c = c.make_current(&s).unwrap();
        HOST.lock().unwrap().fail_discard = true;
        assert!(c.make_not_current_in_place().is_err());
        assert!(!c.is_current());
        assert!(c.make_current(&s).is_err());
        assert!(s.swap_buffers(&c).is_err());
    }
    assert!(HOST.lock().unwrap().leases.is_empty());
}

#[unsafe(no_mangle)]
unsafe extern "C" fn trueos_cabi_ui4_scene_font_sprite_status_v1(
    window: u32,
    ticket: u64,
    out: *mut vcabi::FontSpriteStatus,
) -> i32 {
    if window != 1 || ticket != 17 || out.is_null() {
        return -9;
    }
    unsafe {
        out.write(vcabi::FontSpriteStatus {
            state: 2,
            sprite: 9,
            width: 8,
            height: 11,
            origin_x: -1,
            origin_y: 4,
        });
    }
    0
}

#[test]
fn native_font_import_validates_owner_and_resource_without_readback() {
    let _guard = fixture();
    let d = display();
    let (c, s) = setup(&d, 1);
    assert!(gl::import_font_sprite(1, 17, 9, 8, 11).is_err());
    let c = c.make_current(&s).unwrap();
    assert!(gl::import_font_sprite(2, 17, 9, 8, 11).is_err());
    assert!(gl::import_font_sprite(1, 17, 10, 8, 11).is_err());
    assert!(gl::import_font_sprite(1, 17, 9, 9, 11).is_err());
    let texture = gl::import_font_sprite(1, 17, 9, 8, 11).unwrap();
    assert_eq!(texture, gl::import_font_sprite(1, 17, 9, 8, 11).unwrap());
    unsafe {
        let delete: unsafe extern "system" fn(i32, *const u32) =
            std::mem::transmute(d.get_proc_address(c"glDeleteTextures"));
        delete(1, &texture);
    }
    assert_ne!(texture, gl::import_font_sprite(1, 17, 9, 8, 11).unwrap());
    c.make_not_current().unwrap();
    assert!(gl::import_font_sprite(1, 17, 9, 8, 11).is_err());
}
