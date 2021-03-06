// Copyright 2020 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

//! rutabaga_utils: Utility enums, structs, and implementations needed by the rest of the crate.

use std::fmt::{self, Display};
use std::fs::File;
use std::os::raw::c_void;
use std::path::PathBuf;

use base::ExternalMappingError;
use data_model::VolatileMemoryError;

/// Represents a buffer.  `base` contains the address of a buffer, while `len` contains the length
/// of the buffer.
pub struct RutabagaIovec {
    pub base: *mut c_void,
    pub len: usize,
}

/// 3D resource creation parameters.  Also used to create 2D resource.  Constants based on Mesa's
/// (internal) Gallium interface.  Not in the virtio-gpu spec, but should be since dumb resources
/// can't work with gfxstream/virglrenderer without this.
pub const RUTABAGA_PIPE_TEXTURE_2D: u32 = 2;
pub const RUTABAGA_PIPE_BIND_RENDER_TARGET: u32 = 2;
#[derive(Copy, Clone, Debug)]
pub struct ResourceCreate3D {
    pub target: u32,
    pub format: u32,
    pub bind: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_size: u32,
    pub last_level: u32,
    pub nr_samples: u32,
    pub flags: u32,
}

/// Blob resource creation parameters.
pub const RUTABAGA_BLOB_MEM_GUEST: u32 = 0x0001;
pub const RUTABAGA_BLOB_MEM_HOST3D: u32 = 0x0002;
pub const RUTABAGA_BLOB_MEM_HOST3D_GUEST: u32 = 0x0003;

pub const RUTABAGA_BLOB_FLAG_USE_MAPPABLE: u32 = 0x0001;
pub const RUTABAGA_BLOB_FLAG_USE_SHAREABLE: u32 = 0x0002;
pub const RUTABAGA_BLOB_FLAG_USE_CROSS_DEVICE: u32 = 0x0004;
#[derive(Copy, Clone, Debug)]
pub struct ResourceCreateBlob {
    pub blob_mem: u32,
    pub blob_flags: u32,
    pub blob_id: u64,
    pub size: u64,
}

/// Metadata associated with a swapchain, video or camera image.
#[derive(Debug)]
pub struct Resource3DMetadata {
    pub width: u32,
    pub height: u32,
    pub drm_fourcc: u32,
    pub strides: [u32; 4],
    pub offsets: [u32; 4],
    pub modifier: u64,
}

/// Rutabaga context init capset id mask (not upstreamed).
pub const RUTABAGA_CONTEXT_INIT_CAPSET_ID_MASK: u32 = 0x00ff;

/// Rutabaga flags for creating fences (fence ctx idx info not upstreamed).
pub const RUTABAGA_FLAG_FENCE: u32 = 1 << 0;
pub const RUTABAGA_FLAG_INFO_FENCE_CTX_IDX: u32 = 1 << 1;

/// Convenience struct for Rutabaga fences
pub struct RutabagaFenceData {
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    pub fence_ctx_idx: u32,
}

/// Rutabaga capsets.
pub const RUTABAGA_CAPSET_VIRGL: u32 = 1;
pub const RUTABAGA_CAPSET_VIRGL2: u32 = 2;
/// The following capsets are not upstreamed.
pub const RUTABAGA_CAPSET_GFXSTREAM: u32 = 3;
pub const RUTABAGA_CAPSET_VENUS: u32 = 4;
pub const RUTABAGA_CAPSET_CROSS_DOMAIN: u32 = 5;

/// An error generated while using this crate.
#[derive(Debug)]
pub enum RutabagaError {
    /// Indicates `Rutabaga` was already initialized since only one Rutabaga instance per process
    /// is allowed.
    AlreadyInitialized,
    /// Checked Arithmetic error
    CheckedArithmetic {
        field1: (&'static str, usize),
        field2: (&'static str, usize),
        op: &'static str,
    },
    /// Checked Range error
    CheckedRange {
        field1: (&'static str, usize),
        field2: (&'static str, usize),
    },
    /// The Rutabaga component failed to export a RutabagaHandle.
    ExportedRutabagaHandle,
    /// A command size was submitted that was invalid.
    InvalidCommandSize(usize),
    /// Invalid Context ID
    InvalidContextId,
    /// The indicated region of guest memory is invalid.
    InvalidIovec,
    /// Invalid Resource ID.
    InvalidResourceId,
    /// Indicates an error in the RutabagaBuilder.
    InvalidRutabagaBuild,
    /// The mapping failed.
    MappingFailed(ExternalMappingError),
    /// Memory copy failure.
    MemCopy(VolatileMemoryError),
    /// An internal Rutabaga component error was returned.
    RutabagaComponentError(i32),
    /// The command is unsupported.
    Unsupported,
}

impl Display for RutabagaError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::RutabagaError::*;
        match self {
            AlreadyInitialized => write!(f, "rutabaga component was already initailized"),
            CheckedArithmetic {
                field1: (label1, value1),
                field2: (label2, value2),
                op,
            } => write!(
                f,
                "arithmetic failed: {}({}) {} {}({})",
                label1, value1, op, label2, value2
            ),
            CheckedRange {
                field1: (label1, value1),
                field2: (label2, value2),
            } => write!(
                f,
                "range check failed: {}({}) vs {}({})",
                label1, value1, label2, value2
            ),
            ExportedRutabagaHandle => write!(f, "failed to export Rutabaga handle"),
            InvalidCommandSize(s) => write!(f, "command buffer submitted with invalid size: {}", s),
            InvalidContextId => write!(f, "invalid context id"),
            InvalidIovec => write!(f, "an iovec is outside of guest memory's range"),
            InvalidResourceId => write!(f, "invalid resource id"),
            InvalidRutabagaBuild => write!(f, "invalid rutabaga build parameters"),
            MappingFailed(s) => write!(f, "The mapping failed for the following reason: {}", s),
            MemCopy(e) => write!(f, "{}", e),
            RutabagaComponentError(ret) => write!(f, "rutabaga failed with error {}", ret),
            Unsupported => write!(f, "gpu renderer function unsupported"),
        }
    }
}

/// The result of an operation in this crate.
pub type RutabagaResult<T> = std::result::Result<T, RutabagaError>;

/// Flags for virglrenderer.  Copied from virglrenderer bindings.
pub const VIRGLRENDERER_USE_EGL: u32 = 1;
pub const VIRGLRENDERER_THREAD_SYNC: u32 = 2;
pub const VIRGLRENDERER_USE_GLX: u32 = 4;
pub const VIRGLRENDERER_USE_SURFACELESS: u32 = 8;
pub const VIRGLRENDERER_USE_GLES: u32 = 16;
pub const VIRGLRENDERER_USE_EXTERNAL_BLOB: u32 = 32;

/// virglrenderer flag struct.
#[derive(Copy, Clone)]
pub struct VirglRendererFlags(u32);

impl Default for VirglRendererFlags {
    fn default() -> VirglRendererFlags {
        VirglRendererFlags::new()
            .use_egl(true)
            .use_surfaceless(true)
            .use_gles(true)
    }
}

impl From<VirglRendererFlags> for i32 {
    fn from(flags: VirglRendererFlags) -> i32 {
        flags.0 as i32
    }
}

impl VirglRendererFlags {
    /// Create new virglrenderer flags.
    pub fn new() -> VirglRendererFlags {
        VirglRendererFlags(0)
    }

    fn set_flag(self, bitmask: u32, set: bool) -> VirglRendererFlags {
        if set {
            VirglRendererFlags(self.0 | bitmask)
        } else {
            VirglRendererFlags(self.0 & (!bitmask))
        }
    }

    /// Use EGL for context creation.
    pub fn use_egl(self, v: bool) -> VirglRendererFlags {
        self.set_flag(VIRGLRENDERER_USE_EGL, v)
    }

    /// Use GLX for context creation.
    pub fn use_glx(self, v: bool) -> VirglRendererFlags {
        self.set_flag(VIRGLRENDERER_USE_GLX, v)
    }

    /// No surfaces required when creating context.
    pub fn use_surfaceless(self, v: bool) -> VirglRendererFlags {
        self.set_flag(VIRGLRENDERER_USE_SURFACELESS, v)
    }

    /// Use GLES drivers.
    pub fn use_gles(self, v: bool) -> VirglRendererFlags {
        self.set_flag(VIRGLRENDERER_USE_GLES, v)
    }

    /// Use external memory when creating blob resources.
    pub fn use_external_blob(self, v: bool) -> VirglRendererFlags {
        self.set_flag(VIRGLRENDERER_USE_EXTERNAL_BLOB, v)
    }
}

/// Flags for the gfxstream renderer.
pub const GFXSTREAM_RENDERER_FLAGS_NO_SYNCFD_BIT: u32 = 1 << 20;
pub const GFXSTREAM_RENDERER_FLAGS_NO_VK_BIT: u32 = 1 << 5;
pub const GFXSTREAM_RENDERER_FLAGS_GUEST_USES_ANGLE: u32 = 1 << 21;

/// gfxstream flag struct.
#[derive(Copy, Clone, Default)]
pub struct GfxstreamFlags(u32);

impl GfxstreamFlags {
    /// Create new gfxstream flags.
    pub fn new() -> GfxstreamFlags {
        GfxstreamFlags(0)
    }

    fn set_flag(self, bitmask: u32, set: bool) -> GfxstreamFlags {
        if set {
            GfxstreamFlags(self.0 | bitmask)
        } else {
            GfxstreamFlags(self.0 & (!bitmask))
        }
    }

    /// Use external synchronization.
    pub fn use_syncfd(self, v: bool) -> GfxstreamFlags {
        self.set_flag(GFXSTREAM_RENDERER_FLAGS_NO_SYNCFD_BIT, !v)
    }

    /// Support using Vulkan.
    pub fn support_vulkan(self, v: bool) -> GfxstreamFlags {
        self.set_flag(GFXSTREAM_RENDERER_FLAGS_NO_VK_BIT, !v)
    }

    /// Use ANGLE as the guest GLES driver.
    pub fn use_guest_angle(self, v: bool) -> GfxstreamFlags {
        self.set_flag(GFXSTREAM_RENDERER_FLAGS_GUEST_USES_ANGLE, v)
    }
}

impl From<GfxstreamFlags> for i32 {
    fn from(flags: GfxstreamFlags) -> i32 {
        flags.0 as i32
    }
}

/// Transfers {to, from} 1D buffers, 2D textures, 3D textures, and cubemaps.
#[derive(Debug)]
pub struct Transfer3D {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub w: u32,
    pub h: u32,
    pub d: u32,
    pub level: u32,
    pub stride: u32,
    pub layer_stride: u32,
    pub offset: u64,
}

impl Transfer3D {
    /// Constructs a 2 dimensional XY box in 3 dimensional space with unit depth and zero
    /// displacement on the Z axis.
    pub fn new_2d(x: u32, y: u32, w: u32, h: u32) -> Transfer3D {
        Transfer3D {
            x,
            y,
            z: 0,
            w,
            h,
            d: 1,
            level: 0,
            stride: 0,
            layer_stride: 0,
            offset: 0,
        }
    }

    /// Returns true if this box represents a volume of zero.
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0 || self.d == 0
    }
}

/// Rutabaga channel types
pub const RUTABAGA_CHANNEL_TYPE_WAYLAND: u32 = 0x0001;
pub const RUTABAGA_CHANNEL_TYPE_CAMERA: u32 = 0x0002;

/// Information needed to open an OS-specific RutabagaConnection (TBD).  Only Linux hosts are
/// considered at the moment.
pub struct RutabagaChannel {
    pub channel: PathBuf,
    pub channel_type: u32,
}

/// Enumeration of possible rutabaga components.
#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum RutabagaComponentType {
    Rutabaga2D,
    VirglRenderer,
    Gfxstream,
    CrossDomain,
}

/// Rutabaga handle types (memory and sync in same namespace)
pub const RUTABAGA_MEM_HANDLE_TYPE_OPAQUE_FD: u32 = 0x0001;
pub const RUTABAGA_MEM_HANDLE_TYPE_DMABUF: u32 = 0x0002;
pub const RUTABAGE_MEM_HANDLE_TYPE_OPAQUE_WIN32: u32 = 0x0003;
pub const RUTABAGA_FENCE_HANDLE_TYPE_OPAQUE_FD: u32 = 0x0004;
pub const RUTABAGA_FENCE_HANDLE_TYPE_SYNC_FD: u32 = 0x0005;
pub const RUTABAGE_FENCE_HANDLE_TYPE_OPAQUE_WIN32: u32 = 0x0006;

/// Handle to OS-specific memory or synchronization objects.
pub struct RutabagaHandle {
    pub os_handle: File,
    pub handle_type: u32,
}

impl RutabagaHandle {
    /// Clones an existing rutabaga handle, by using OS specific mechanisms.
    pub fn try_clone(&self) -> RutabagaResult<RutabagaHandle> {
        let clone = self
            .os_handle
            .try_clone()
            .map_err(|_| RutabagaError::Unsupported)?;
        Ok(RutabagaHandle {
            os_handle: clone,
            handle_type: self.handle_type,
        })
    }
}
