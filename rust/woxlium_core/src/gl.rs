//! Raw OpenGL ES 2.0 bindings (only what the renderer uses).
#![allow(non_snake_case, dead_code)]

use std::os::raw::{c_char, c_float, c_int, c_uint, c_void};

pub type GLenum = c_uint;
pub type GLuint = c_uint;
pub type GLint = c_int;
pub type GLsizei = c_int;
pub type GLfloat = c_float;
pub type GLboolean = u8;
pub type GLbitfield = c_uint;
pub type GLsizeiptr = isize;

pub const DEPTH_BUFFER_BIT: GLbitfield = 0x0000_0100;
pub const COLOR_BUFFER_BIT: GLbitfield = 0x0000_4000;
pub const TRIANGLES: GLenum = 0x0004;
pub const ONE: GLenum = 1;
pub const SRC_ALPHA: GLenum = 0x0302;
pub const ONE_MINUS_SRC_ALPHA: GLenum = 0x0303;
pub const DEPTH_TEST: GLenum = 0x0B71;
pub const BLEND: GLenum = 0x0BE2;
pub const CULL_FACE: GLenum = 0x0B44;
pub const SCISSOR_TEST: GLenum = 0x0C11;
pub const POLYGON_OFFSET_FILL: GLenum = 0x8037;
pub const LEQUAL: GLenum = 0x0203;
pub const GREATER: GLenum = 0x0204;
pub const LESS: GLenum = 0x0201;
pub const UNSIGNED_BYTE: GLenum = 0x1401;
pub const UNSIGNED_SHORT: GLenum = 0x1403;
pub const FLOAT: GLenum = 0x1406;
pub const RGBA: GLenum = 0x1908;
pub const TEXTURE_2D: GLenum = 0x0DE1;
pub const TEXTURE0: GLenum = 0x84C0;
pub const TEXTURE_MAG_FILTER: GLenum = 0x2800;
pub const TEXTURE_MIN_FILTER: GLenum = 0x2801;
pub const TEXTURE_WRAP_S: GLenum = 0x2802;
pub const TEXTURE_WRAP_T: GLenum = 0x2803;
pub const LINEAR: GLint = 0x2601;
pub const LINEAR_MIPMAP_LINEAR: GLint = 0x2703;
pub const REPEAT: GLint = 0x2901;
pub const CLAMP_TO_EDGE: GLint = 0x812F;
pub const ARRAY_BUFFER: GLenum = 0x8892;
pub const ELEMENT_ARRAY_BUFFER: GLenum = 0x8893;
pub const STATIC_DRAW: GLenum = 0x88E4;
pub const DYNAMIC_DRAW: GLenum = 0x88E8;
pub const STREAM_DRAW: GLenum = 0x88E0;
pub const FRAGMENT_SHADER: GLenum = 0x8B30;
pub const VERTEX_SHADER: GLenum = 0x8B31;
pub const COMPILE_STATUS: GLenum = 0x8B81;
pub const LINK_STATUS: GLenum = 0x8B82;
pub const INFO_LOG_LENGTH: GLenum = 0x8B84;
pub const FALSE: GLboolean = 0;
pub const TRUE: GLboolean = 1;

#[cfg_attr(target_os = "android", link(name = "GLESv2"))]
#[cfg_attr(all(not(target_os = "android"), feature = "preview"), link(name = "GLESv2"))]
extern "C" {
    pub fn glActiveTexture(texture: GLenum);
    pub fn glAttachShader(program: GLuint, shader: GLuint);
    pub fn glBindAttribLocation(program: GLuint, index: GLuint, name: *const c_char);
    pub fn glBindBuffer(target: GLenum, buffer: GLuint);
    pub fn glBindTexture(target: GLenum, texture: GLuint);
    pub fn glBlendFunc(sfactor: GLenum, dfactor: GLenum);
    pub fn glBufferData(target: GLenum, size: GLsizeiptr, data: *const c_void, usage: GLenum);
    pub fn glBufferSubData(target: GLenum, offset: isize, size: GLsizeiptr, data: *const c_void);
    pub fn glClear(mask: GLbitfield);
    pub fn glClearColor(r: GLfloat, g: GLfloat, b: GLfloat, a: GLfloat);
    pub fn glCompileShader(shader: GLuint);
    pub fn glCreateProgram() -> GLuint;
    pub fn glCreateShader(kind: GLenum) -> GLuint;
    pub fn glDeleteBuffers(n: GLsizei, buffers: *const GLuint);
    pub fn glDeleteProgram(program: GLuint);
    pub fn glDeleteShader(shader: GLuint);
    pub fn glDeleteTextures(n: GLsizei, textures: *const GLuint);
    pub fn glDepthFunc(func: GLenum);
    pub fn glDepthMask(flag: GLboolean);
    pub fn glDisable(cap: GLenum);
    pub fn glDisableVertexAttribArray(index: GLuint);
    pub fn glDrawArrays(mode: GLenum, first: GLint, count: GLsizei);
    pub fn glDrawElements(mode: GLenum, count: GLsizei, kind: GLenum, indices: *const c_void);
    pub fn glEnable(cap: GLenum);
    pub fn glEnableVertexAttribArray(index: GLuint);
    pub fn glGenBuffers(n: GLsizei, buffers: *mut GLuint);
    pub fn glGenTextures(n: GLsizei, textures: *mut GLuint);
    pub fn glGenerateMipmap(target: GLenum);
    pub fn glGetProgramInfoLog(program: GLuint, max: GLsizei, len: *mut GLsizei, log: *mut c_char);
    pub fn glGetProgramiv(program: GLuint, pname: GLenum, params: *mut GLint);
    pub fn glGetShaderInfoLog(shader: GLuint, max: GLsizei, len: *mut GLsizei, log: *mut c_char);
    pub fn glGetShaderiv(shader: GLuint, pname: GLenum, params: *mut GLint);
    pub fn glGetUniformLocation(program: GLuint, name: *const c_char) -> GLint;
    pub fn glLinkProgram(program: GLuint);
    pub fn glPolygonOffset(factor: GLfloat, units: GLfloat);
    pub fn glReadPixels(x: GLint, y: GLint, w: GLsizei, h: GLsizei, format: GLenum, kind: GLenum, data: *mut c_void);
    pub fn glScissor(x: GLint, y: GLint, w: GLsizei, h: GLsizei);
    pub fn glShaderSource(shader: GLuint, count: GLsizei, string: *const *const c_char, length: *const GLint);
    pub fn glTexImage2D(target: GLenum, level: GLint, internal: GLint, w: GLsizei, h: GLsizei, border: GLint, format: GLenum, kind: GLenum, data: *const c_void);
    pub fn glTexParameteri(target: GLenum, pname: GLenum, param: GLint);
    pub fn glUniform1f(loc: GLint, v: GLfloat);
    pub fn glUniform1i(loc: GLint, v: GLint);
    pub fn glUniform2f(loc: GLint, a: GLfloat, b: GLfloat);
    pub fn glUniform3f(loc: GLint, a: GLfloat, b: GLfloat, c: GLfloat);
    pub fn glUniform4f(loc: GLint, a: GLfloat, b: GLfloat, c: GLfloat, d: GLfloat);
    pub fn glUniformMatrix4fv(loc: GLint, count: GLsizei, transpose: GLboolean, value: *const GLfloat);
    pub fn glUseProgram(program: GLuint);
    pub fn glVertexAttribPointer(index: GLuint, size: GLint, kind: GLenum, normalized: GLboolean, stride: GLsizei, ptr: *const c_void);
    pub fn glViewport(x: GLint, y: GLint, w: GLsizei, h: GLsizei);
}
