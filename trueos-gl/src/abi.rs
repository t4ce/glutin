//! Wire declarations shared with the TRUEOS vGPU/UI4 broker.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct SurfaceInfo {
    pub surface: u64,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub format: u32,
}

pub const SURFACE_FORMAT_RGBA8_UNORM_SRGB: u32 = 1;

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct TimelinePoint {
    pub queue: u64,
    pub value: u64,
    pub physical_serial: u64,
    pub physical_publish_sequence: u64,
}

unsafe extern "C" {
    pub fn trueos_cabi_ui4_display_retain_v1(connection: u64) -> i32;
    pub fn trueos_cabi_ui4_display_close_v1(connection: u64) -> i32;
    pub fn trueos_cabi_ui4_display_validate_window_v1(connection: u64, window: u32) -> i32;
    pub fn trueos_cabi_ui4_scene_frame_resize(window: u32, width: u32, height: u32) -> i32;
    pub fn trueos_cabi_vgpu_queue_create(device: u64, class: u32, out_queue: *mut u64) -> i32;
    pub fn trueos_cabi_vgpu_queue_destroy(device: u64, queue: u64) -> i32;
    pub fn trueos_cabi_vgpu_ui4_surface_clear_submit(
        device: u64,
        queue: u64,
        surface: u64,
        color: u32,
        point: *mut TimelinePoint,
    ) -> i32;
    pub fn trueos_cabi_vgpu_wait(device: u64, queue: u64, value: u64) -> i32;

    pub fn trueos_cabi_vgpu_open(requested_caps: u64, out_device: *mut u64) -> i32;
    pub fn trueos_cabi_vgpu_close(device: u64) -> i32;
    pub fn trueos_cabi_vgpu_ui4_surface_acquire(
        device: u64,
        window_id: u32,
        out: *mut SurfaceInfo,
    ) -> i32;
    pub fn trueos_cabi_vgpu_ui4_surface_discard(device: u64, surface: u64) -> i32;
}

#[repr(C)]
#[derive(Default)]
pub struct FontSpriteStatus {
    pub state: u32,
    pub sprite: u32,
    pub width: u32,
    pub height: u32,
    pub origin_x: i32,
    pub origin_y: i32,
}
unsafe extern "C" {
    pub fn trueos_cabi_ui4_scene_font_sprite_status_v1(
        window: u32,
        ticket: u64,
        out: *mut FontSpriteStatus,
    ) -> i32;
}
