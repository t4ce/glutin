//! The bounded resource/state subset used by Alacritty's GLES2Pure renderer.
use super::{INVALID_ENUM, INVALID_OPERATION, INVALID_VALUE, with_state};
use std::collections::BTreeMap;
use std::ffi::{CStr, c_void};

const ARRAY: u32 = 0x8892;
const ELEMENT: u32 = 0x8893;
const TEXTURE: u32 = 0xDE1;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const OUT_OF_MEMORY: u32 = 0x505;
#[derive(Debug, Default, Clone)]
struct Attribute {
    buffer: u32,
    count: usize,
    scalar: usize,
    stride: usize,
    offset: usize,
    normalized: bool,
    kind: u32,
    enabled: bool,
}
#[derive(Debug, Default)]
struct VertexArray {
    elements: u32,
    attributes: BTreeMap<u32, Attribute>,
}
#[derive(Debug, Default)]
struct Texture {
    width: usize,
    height: usize,
    channels: usize,
    data: Vec<u8>,
    params: BTreeMap<u32, i32>,
    native: Option<(u32, u64, u32)>,
}
#[derive(Debug)]
pub(super) struct Objects {
    next: u32,
    buffers: BTreeMap<u32, Vec<u8>>,
    arrays: BTreeMap<u32, VertexArray>,
    textures: BTreeMap<u32, Texture>,
    array: u32,
    vao: u32,
    texture: u32,
    unpack: usize,
    program: u32,
    uniforms: BTreeMap<(u32, i32), [f32; 4]>,
    blend: [u32; 4],
    blending: bool,
}
impl Default for Objects {
    fn default() -> Self {
        Self {
            next: 1,
            buffers: BTreeMap::new(),
            arrays: BTreeMap::from([(0, VertexArray::default())]),
            textures: BTreeMap::new(),
            array: 0,
            vao: 0,
            texture: 0,
            unpack: 4,
            program: 0,
            uniforms: BTreeMap::new(),
            blend: [1, 0, 1, 0],
            blending: false,
        }
    }
}
impl Objects {
    fn buffer(&self, target: u32) -> Result<u32, u32> {
        match target {
            ARRAY => Ok(self.array),
            ELEMENT => Ok(self.arrays[&self.vao].elements),
            _ => Err(INVALID_ENUM),
        }
    }

    fn bytes(&self) -> usize {
        self.buffers.values().map(Vec::len).sum::<usize>()
            + self.textures.values().map(|t| t.data.len()).sum::<usize>()
    }
}
fn mutate(f: impl FnOnce(&mut Objects) -> Result<(), u32>) {
    with_state((), |s| {
        if let Err(e) = f(&mut s.objects) {
            s.error(e)
        }
    });
}
fn allocate(len: usize) -> Result<Vec<u8>, u32> {
    if len > MAX_BYTES {
        return Err(OUT_OF_MEMORY);
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).map_err(|_| OUT_OF_MEMORY)?;
    bytes.resize(len, 0);
    Ok(bytes)
}
unsafe fn names(n: i32, p: *mut u32, kind: u32) {
    mutate(|s| {
        if n < 0 || (n > 0 && p.is_null()) {
            return Err(INVALID_VALUE);
        }
        let end = s.next.checked_add(n as u32).ok_or(OUT_OF_MEMORY)?;
        if end > 65536 {
            return Err(OUT_OF_MEMORY);
        }
        for i in 0..n as usize {
            let name = s.next;
            s.next += 1;
            match kind {
                ARRAY => {
                    s.buffers.insert(name, Vec::new());
                },
                TEXTURE => {
                    s.textures.insert(name, Texture::default());
                },
                _ => {
                    s.arrays.insert(name, VertexArray::default());
                },
            }
            unsafe { p.add(i).write(name) }
        }
        Ok(())
    });
}
unsafe extern "system" fn gen_buffers(n: i32, p: *mut u32) {
    unsafe { names(n, p, ARRAY) }
}
unsafe extern "system" fn gen_arrays(n: i32, p: *mut u32) {
    unsafe { names(n, p, 0) }
}
unsafe extern "system" fn gen_textures(n: i32, p: *mut u32) {
    unsafe { names(n, p, TEXTURE) }
}
unsafe fn delete(n: i32, p: *const u32, kind: u32) {
    mutate(|s| {
        if n < 0 || (n > 0 && p.is_null()) {
            return Err(INVALID_VALUE);
        }
        for i in 0..n as usize {
            let name = unsafe { p.add(i).read() };
            if name == 0 {
                continue;
            }
            match kind {
                ARRAY => {
                    s.buffers.remove(&name);
                    if s.array == name {
                        s.array = 0
                    }
                    for array in s.arrays.values_mut() {
                        if array.elements == name {
                            array.elements = 0
                        }
                        for attr in array.attributes.values_mut() {
                            if attr.buffer == name {
                                attr.buffer = 0
                            }
                        }
                    }
                },
                TEXTURE => {
                    s.textures.remove(&name);
                    if s.texture == name {
                        s.texture = 0
                    }
                },
                _ => {
                    s.arrays.remove(&name);
                    if s.vao == name {
                        s.vao = 0
                    }
                },
            }
        }
        Ok(())
    });
}
unsafe extern "system" fn delete_buffers(n: i32, p: *const u32) {
    unsafe { delete(n, p, ARRAY) }
}
unsafe extern "system" fn delete_arrays(n: i32, p: *const u32) {
    unsafe { delete(n, p, 0) }
}
unsafe extern "system" fn delete_textures(n: i32, p: *const u32) {
    unsafe { delete(n, p, TEXTURE) }
}
unsafe extern "system" fn bind_buffer(target: u32, name: u32) {
    mutate(|s| {
        if target != ARRAY && target != ELEMENT {
            return Err(INVALID_ENUM);
        }
        if name != 0 && !s.buffers.contains_key(&name) {
            return Err(INVALID_OPERATION);
        }
        if target == ARRAY {
            s.array = name
        } else {
            s.arrays.get_mut(&s.vao).unwrap().elements = name
        }
        Ok(())
    });
}
unsafe extern "system" fn bind_array(name: u32) {
    mutate(|s| {
        if !s.arrays.contains_key(&name) {
            return Err(INVALID_OPERATION);
        }
        s.vao = name;
        Ok(())
    });
}
unsafe extern "system" fn buffer_data(target: u32, len: isize, data: *const c_void, usage: u32) {
    mutate(|s| {
        if len < 0 {
            return Err(INVALID_VALUE);
        }
        if !matches!(usage, 0x88E0 | 0x88E4 | 0x88E8) {
            return Err(INVALID_ENUM);
        }
        let id = s.buffer(target)?;
        let old = s.buffers.get(&id).ok_or(INVALID_OPERATION)?.len();
        if s.bytes() - old + len as usize > MAX_BYTES * 4 {
            return Err(OUT_OF_MEMORY);
        }
        let mut bytes = allocate(len as usize)?;
        if !data.is_null() && !bytes.is_empty() {
            unsafe { std::ptr::copy_nonoverlapping(data.cast(), bytes.as_mut_ptr(), bytes.len()) }
        }
        s.buffers.insert(id, bytes);
        Ok(())
    });
}
unsafe extern "system" fn buffer_sub_data(
    target: u32,
    offset: isize,
    len: isize,
    data: *const c_void,
) {
    mutate(|s| {
        if offset < 0 || len < 0 || (len > 0 && data.is_null()) {
            return Err(INVALID_VALUE);
        }
        let id = s.buffer(target)?;
        let buffer = s.buffers.get_mut(&id).ok_or(INVALID_OPERATION)?;
        let end = (offset as usize).checked_add(len as usize).ok_or(INVALID_VALUE)?;
        let slice = buffer.get_mut(offset as usize..end).ok_or(INVALID_VALUE)?;
        if !slice.is_empty() {
            unsafe { std::ptr::copy_nonoverlapping(data.cast(), slice.as_mut_ptr(), slice.len()) }
        }
        Ok(())
    });
}
unsafe extern "system" fn attribute(
    index: u32,
    count: i32,
    kind: u32,
    normalized: u8,
    stride: i32,
    offset: *const c_void,
) {
    mutate(|s| {
        if index >= 16 || !(1..=4).contains(&count) || stride < 0 || normalized > 1 {
            return Err(INVALID_VALUE);
        }
        let scalar = match kind {
            0x1400 | 0x1401 => 1,
            0x1402 | 0x1403 => 2,
            0x1406 => 4,
            _ => return Err(INVALID_ENUM),
        };
        if s.array == 0 {
            return Err(INVALID_OPERATION);
        }
        let attrs = &mut s.arrays.get_mut(&s.vao).unwrap().attributes;
        let enabled = attrs.get(&index).is_some_and(|a| a.enabled);
        attrs.insert(
            index,
            Attribute {
                buffer: s.array,
                count: count as usize,
                scalar,
                stride: if stride == 0 { count as usize * scalar } else { stride as usize },
                offset: offset as usize,
                normalized: normalized != 0,
                kind,
                enabled,
            },
        );
        Ok(())
    });
}
unsafe extern "system" fn enable_attribute(index: u32) {
    mutate(|s| {
        if index >= 16 {
            return Err(INVALID_VALUE);
        }
        s.arrays.get_mut(&s.vao).unwrap().attributes.entry(index).or_default().enabled = true;
        Ok(())
    });
}
unsafe extern "system" fn active_texture(unit: u32) {
    mutate(|_| if unit == 0x84C0 { Ok(()) } else { Err(INVALID_ENUM) });
}
unsafe extern "system" fn bind_texture(target: u32, id: u32) {
    mutate(|s| {
        if target != TEXTURE {
            return Err(INVALID_ENUM);
        }
        if id != 0 && !s.textures.contains_key(&id) {
            return Err(INVALID_OPERATION);
        }
        s.texture = id;
        Ok(())
    });
}
unsafe extern "system" fn pixel_store(name: u32, value: i32) {
    mutate(|s| {
        if name != 0xCF5 {
            return Err(INVALID_ENUM);
        }
        if !matches!(value, 1 | 2 | 4 | 8) {
            return Err(INVALID_VALUE);
        }
        s.unpack = value as usize;
        Ok(())
    });
}
unsafe extern "system" fn texture_parameter(target: u32, name: u32, value: i32) {
    mutate(|s| {
        if target != TEXTURE {
            return Err(INVALID_ENUM);
        }
        let valid = match name {
            0x2800 | 0x2801 => matches!(value, 0x2600 | 0x2601),
            0x2802 | 0x2803 => value == 0x812F,
            _ => false,
        };
        if !valid {
            return Err(INVALID_ENUM);
        }
        s.textures.get_mut(&s.texture).ok_or(INVALID_OPERATION)?.params.insert(name, value);
        Ok(())
    });
}
fn channels(format: u32) -> Result<usize, u32> {
    match format {
        0x1907 => Ok(3),
        0x1908 => Ok(4),
        _ => Err(INVALID_ENUM),
    }
}
unsafe extern "system" fn texture_image(
    target: u32,
    level: i32,
    internal: i32,
    width: i32,
    height: i32,
    border: i32,
    format: u32,
    kind: u32,
    data: *const c_void,
) {
    mutate(|s| {
        if target != TEXTURE || kind != 0x1401 {
            return Err(INVALID_ENUM);
        }
        if level != 0
            || border != 0
            || width < 0
            || height < 0
            || width > 4096
            || height > 4096
            || internal != format as i32
        {
            return Err(INVALID_VALUE);
        }
        let channels = channels(format)?;
        let width = width as usize;
        let height = height as usize;
        let old = s.textures.get(&s.texture).ok_or(INVALID_OPERATION)?.data.len();
        let len = width * height * channels;
        if s.bytes() - old + len > MAX_BYTES * 4 {
            return Err(OUT_OF_MEMORY);
        }
        let mut bytes = allocate(len)?;
        let row = width * channels;
        let pitch = row.next_multiple_of(s.unpack);
        if !data.is_null() && row > 0 {
            for y in 0..height {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        data.cast::<u8>().add(y * pitch),
                        bytes.as_mut_ptr().add(y * row),
                        row,
                    )
                }
            }
        }
        let texture = s.textures.get_mut(&s.texture).unwrap();
        texture.native = None;
        texture.width = width;
        texture.height = height;
        texture.channels = channels;
        texture.data = bytes;
        Ok(())
    });
}
unsafe extern "system" fn texture_sub_image(
    target: u32,
    level: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    format: u32,
    kind: u32,
    data: *const c_void,
) {
    mutate(|s| {
        if target != TEXTURE || kind != 0x1401 {
            return Err(INVALID_ENUM);
        }
        if level != 0 || x < 0 || y < 0 || width < 0 || height < 0 {
            return Err(INVALID_VALUE);
        }
        let channels = channels(format)?;
        let texture = s.textures.get_mut(&s.texture).ok_or(INVALID_OPERATION)?;
        let (x, y, width, height) = (x as usize, y as usize, width as usize, height as usize);
        if texture.native.is_some() {
            return Err(INVALID_OPERATION);
        }
        if x + width > texture.width || y + height > texture.height {
            return Err(INVALID_VALUE);
        }
        if channels != texture.channels {
            return Err(INVALID_OPERATION);
        }
        if width == 0 || height == 0 {
            return Ok(());
        }
        if data.is_null() {
            return Err(INVALID_VALUE);
        }
        let row = width * channels;
        let pitch = row.next_multiple_of(s.unpack);
        for offset in 0..height {
            let start = ((y + offset) * texture.width + x) * channels;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    data.cast::<u8>().add(offset * pitch),
                    texture.data.as_mut_ptr().add(start),
                    row,
                )
            }
        }
        Ok(())
    });
}
unsafe extern "system" fn use_program(id: u32) {
    mutate(|s| {
        if id != 0 && !(0x54520001..=0x54520005).contains(&id) {
            return Err(INVALID_VALUE);
        }
        s.program = id;
        Ok(())
    });
}
unsafe extern "system" fn delete_program(id: u32) {
    mutate(|_| {
        if id == 0 || (0x54520001..=0x54520005).contains(&id) { Ok(()) } else { Err(INVALID_VALUE) }
    });
}
fn uniform(location: i32, value: [f32; 4]) {
    mutate(|s| {
        if location == -1 {
            return Ok(());
        }
        let limit = if s.program == 0x54520001 { 3 } else { 7 };
        if s.program == 0 {
            return Err(INVALID_OPERATION);
        }
        if location < 0 || location >= limit {
            return Err(INVALID_OPERATION);
        }
        s.uniforms.insert((s.program, location), value);
        Ok(())
    });
}
unsafe extern "system" fn uniform1f(location: i32, x: f32) {
    uniform(location, [x, 0., 0., 0.]);
}
unsafe extern "system" fn uniform1i(location: i32, x: i32) {
    uniform(location, [x as f32, 0., 0., 0.]);
}
unsafe extern "system" fn uniform4f(location: i32, x: f32, y: f32, z: f32, w: f32) {
    uniform(location, [x, y, z, w]);
}
fn blend_valid(f: u32) -> bool {
    matches!(f, 0 | 1 | 0x300..=0x307)
}
unsafe extern "system" fn blend_separate(src: u32, dst: u32, src_alpha: u32, dst_alpha: u32) {
    mutate(|s| {
        let blend = [src, dst, src_alpha, dst_alpha];
        if !blend.into_iter().all(blend_valid) {
            return Err(INVALID_ENUM);
        }
        s.blend = blend;
        Ok(())
    });
}
unsafe extern "system" fn blend(src: u32, dst: u32) {
    unsafe { blend_separate(src, dst, src, dst) }
}
unsafe extern "system" fn enable(cap: u32) {
    mutate(|s| {
        if cap == 0xBE2 {
            s.blending = true;
            Ok(())
        } else {
            Err(INVALID_ENUM)
        }
    });
}
unsafe extern "system" fn depth_mask(value: u8) {
    mutate(|_| if value <= 1 { Ok(()) } else { Err(INVALID_VALUE) });
}
unsafe extern "system" fn get_integer(name: u32, out: *mut i32) {
    with_state((), |s| {
        if out.is_null() {
            s.error(INVALID_VALUE);
            return;
        }
        let value = match name {
            0xD33 => 4096,
            0x8869 => 16,
            0x8872 => 1,
            0xCF5 => s.objects.unpack as i32,
            _ => {
                s.error(INVALID_ENUM);
                return;
            },
        };
        unsafe { out.write(value) }
    });
}
fn draw(max_vertex: Result<Option<usize>, u32>) {
    with_state((), |state| {
        let result = (|| {
            let Some(max) = max_vertex? else { return Ok(()) };
            let s = &state.objects;
            if s.program == 0 {
                return Err(INVALID_OPERATION);
            }
            let array = &s.arrays[&s.vao];
            let count = if s.program == 0x54520001 { 5 } else { 2 };
            for index in 0..count {
                let a =
                    array.attributes.get(&index).filter(|a| a.enabled).ok_or(INVALID_OPERATION)?;
                let expected = if s.program == 0x54520001 {
                    match index {
                        0 | 1 => (2, 0x1402, false),
                        2 => (2, 0x1406, false),
                        _ => (4, 0x1401, false),
                    }
                } else if index == 0 {
                    (2, 0x1406, false)
                } else {
                    (4, 0x1401, true)
                };
                if (a.count, a.kind, a.normalized) != expected {
                    return Err(INVALID_OPERATION);
                }
                let buffer = s.buffers.get(&a.buffer).ok_or(INVALID_OPERATION)?;
                let end = max
                    .checked_mul(a.stride)
                    .and_then(|v| v.checked_add(a.offset))
                    .and_then(|v| v.checked_add(a.count * a.scalar))
                    .ok_or(INVALID_OPERATION)?;
                if end > buffer.len() {
                    return Err(INVALID_OPERATION);
                }
            }
            if s.program == 0x54520001
                && s.textures
                    .get(&s.texture)
                    .is_none_or(|t| t.data.is_empty() && t.native.is_none())
            {
                return Err(INVALID_OPERATION);
            }
            // Retain the failure at the frame boundary. Missing native execution
            // must never publish the earlier clear as if the text draw succeeded.
            state.pending_draw = true;
            Ok(())
        })();
        if let Err(error) = result {
            state.error(error);
            state.pending_draw = true;
        }
    });
}
unsafe extern "system" fn draw_arrays(mode: u32, first: i32, count: i32) {
    draw(if mode != 4 {
        Err(INVALID_ENUM)
    } else if first < 0 || count < 0 {
        Err(INVALID_VALUE)
    } else if count == 0 {
        Ok(None)
    } else {
        Ok(Some(first as usize + count as usize - 1))
    });
}
unsafe extern "system" fn draw_elements(mode: u32, count: i32, kind: u32, offset: *const c_void) {
    let range = with_state(Err(INVALID_OPERATION), |state| {
        if mode != 4 || kind != 0x1403 {
            return Err(INVALID_ENUM);
        }
        if count < 0 {
            return Err(INVALID_VALUE);
        }
        if count == 0 {
            return Ok(None);
        }
        let s = &state.objects;
        let id = s.arrays[&s.vao].elements;
        let buffer = s.buffers.get(&id).ok_or(INVALID_OPERATION)?;
        let start = offset as usize;
        if start % 2 != 0 {
            return Err(INVALID_OPERATION);
        }
        let end = start.checked_add(count as usize * 2).ok_or(INVALID_OPERATION)?;
        let bytes = buffer.get(start..end).ok_or(INVALID_OPERATION)?;
        Ok(bytes.chunks_exact(2).map(|b| u16::from_ne_bytes([b[0], b[1]]) as usize).max())
    });
    draw(range);
}

pub(super) fn resolve(name: &CStr) -> *const c_void {
    macro_rules! entries {($($name:literal=>$func:ident),*$(,)?)=>{match name.to_bytes(){$($name=>$func as *const c_void,)*_=>std::ptr::null()}}}
    entries! {
        b"glGenBuffers"=>gen_buffers,b"glGenVertexArrays"=>gen_arrays,b"glGenVertexArraysOES"=>gen_arrays,
        b"glGenTextures"=>gen_textures,b"glDeleteBuffers"=>delete_buffers,b"glDeleteVertexArrays"=>delete_arrays,
        b"glDeleteVertexArraysOES"=>delete_arrays,b"glDeleteTextures"=>delete_textures,b"glBindBuffer"=>bind_buffer,
        b"glBindVertexArray"=>bind_array,b"glBindVertexArrayOES"=>bind_array,b"glBufferData"=>buffer_data,
        b"glBufferSubData"=>buffer_sub_data,b"glVertexAttribPointer"=>attribute,b"glEnableVertexAttribArray"=>enable_attribute,
        b"glActiveTexture"=>active_texture,b"glBindTexture"=>bind_texture,b"glPixelStorei"=>pixel_store,
        b"glTexParameteri"=>texture_parameter,b"glTexImage2D"=>texture_image,b"glTexSubImage2D"=>texture_sub_image,
        b"glUseProgram"=>use_program,b"glDeleteProgram"=>delete_program,b"glUniform1f"=>uniform1f,
        b"glUniform1i"=>uniform1i,b"glUniform4f"=>uniform4f,b"glBlendFunc"=>blend,
        b"glBlendFuncSeparate"=>blend_separate,b"glEnable"=>enable,b"glDepthMask"=>depth_mask,
        b"glGetIntegerv"=>get_integer,b"glDrawArrays"=>draw_arrays,b"glDrawElements"=>draw_elements
    }
}

/// Register an already validated window-owned glyph, without copying its pixels.
pub(super) fn import_font(
    window: u32,
    ticket: u64,
    sprite: u32,
    width: u32,
    height: u32,
) -> Result<u32, u32> {
    with_state(Err(INVALID_OPERATION), |state| {
        let s = &mut state.objects;
        if let Some((id, _)) =
            s.textures.iter().find(|(_, t)| t.native == Some((window, ticket, sprite)))
        {
            return Ok(*id);
        }
        let id = s.next;
        s.next = id.checked_add(1).ok_or(OUT_OF_MEMORY)?;
        s.textures.insert(
            id,
            Texture {
                width: width as usize,
                height: height as usize,
                native: Some((window, ticket, sprite)),
                ..Default::default()
            },
        );
        Ok(id)
    })
}
