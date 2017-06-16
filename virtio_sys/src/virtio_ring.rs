/* automatically generated by rust-bindgen */

#[repr(C)]
#[derive(Default)]
pub struct __IncompleteArrayField<T>(::std::marker::PhantomData<T>);
impl <T> __IncompleteArrayField<T> {
    #[inline]
    pub fn new() -> Self {
        __IncompleteArrayField(::std::marker::PhantomData)
    }
    #[inline]
    pub unsafe fn as_ptr(&self) -> *const T { ::std::mem::transmute(self) }
    #[inline]
    pub unsafe fn as_mut_ptr(&mut self) -> *mut T {
        ::std::mem::transmute(self)
    }
    #[inline]
    pub unsafe fn as_slice(&self, len: usize) -> &[T] {
        ::std::slice::from_raw_parts(self.as_ptr(), len)
    }
    #[inline]
    pub unsafe fn as_mut_slice(&mut self, len: usize) -> &mut [T] {
        ::std::slice::from_raw_parts_mut(self.as_mut_ptr(), len)
    }
}
impl <T> ::std::fmt::Debug for __IncompleteArrayField<T> {
    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        fmt.write_str("__IncompleteArrayField")
    }
}
impl <T> ::std::clone::Clone for __IncompleteArrayField<T> {
    #[inline]
    fn clone(&self) -> Self { Self::new() }
}
impl <T> ::std::marker::Copy for __IncompleteArrayField<T> { }
pub const _STDINT_H: ::std::os::raw::c_uint = 1;
pub const _FEATURES_H: ::std::os::raw::c_uint = 1;
pub const _DEFAULT_SOURCE: ::std::os::raw::c_uint = 1;
pub const __USE_ISOC11: ::std::os::raw::c_uint = 1;
pub const __USE_ISOC99: ::std::os::raw::c_uint = 1;
pub const __USE_ISOC95: ::std::os::raw::c_uint = 1;
pub const __USE_POSIX_IMPLICITLY: ::std::os::raw::c_uint = 1;
pub const _POSIX_SOURCE: ::std::os::raw::c_uint = 1;
pub const _POSIX_C_SOURCE: ::std::os::raw::c_uint = 200809;
pub const __USE_POSIX: ::std::os::raw::c_uint = 1;
pub const __USE_POSIX2: ::std::os::raw::c_uint = 1;
pub const __USE_POSIX199309: ::std::os::raw::c_uint = 1;
pub const __USE_POSIX199506: ::std::os::raw::c_uint = 1;
pub const __USE_XOPEN2K: ::std::os::raw::c_uint = 1;
pub const __USE_XOPEN2K8: ::std::os::raw::c_uint = 1;
pub const _ATFILE_SOURCE: ::std::os::raw::c_uint = 1;
pub const __USE_MISC: ::std::os::raw::c_uint = 1;
pub const __USE_ATFILE: ::std::os::raw::c_uint = 1;
pub const __USE_FORTIFY_LEVEL: ::std::os::raw::c_uint = 0;
pub const _STDC_PREDEF_H: ::std::os::raw::c_uint = 1;
pub const __STDC_IEC_559__: ::std::os::raw::c_uint = 1;
pub const __STDC_IEC_559_COMPLEX__: ::std::os::raw::c_uint = 1;
pub const __STDC_ISO_10646__: ::std::os::raw::c_uint = 201505;
pub const __STDC_NO_THREADS__: ::std::os::raw::c_uint = 1;
pub const __GNU_LIBRARY__: ::std::os::raw::c_uint = 6;
pub const __GLIBC__: ::std::os::raw::c_uint = 2;
pub const __GLIBC_MINOR__: ::std::os::raw::c_uint = 23;
pub const _SYS_CDEFS_H: ::std::os::raw::c_uint = 1;
pub const __WORDSIZE: ::std::os::raw::c_uint = 64;
pub const __WORDSIZE_TIME64_COMPAT32: ::std::os::raw::c_uint = 1;
pub const __SYSCALL_WORDSIZE: ::std::os::raw::c_uint = 64;
pub const _BITS_WCHAR_H: ::std::os::raw::c_uint = 1;
pub const INT8_MIN: ::std::os::raw::c_int = -128;
pub const INT16_MIN: ::std::os::raw::c_int = -32768;
pub const INT32_MIN: ::std::os::raw::c_int = -2147483648;
pub const INT8_MAX: ::std::os::raw::c_uint = 127;
pub const INT16_MAX: ::std::os::raw::c_uint = 32767;
pub const INT32_MAX: ::std::os::raw::c_uint = 2147483647;
pub const UINT8_MAX: ::std::os::raw::c_uint = 255;
pub const UINT16_MAX: ::std::os::raw::c_uint = 65535;
pub const UINT32_MAX: ::std::os::raw::c_uint = 4294967295;
pub const INT_LEAST8_MIN: ::std::os::raw::c_int = -128;
pub const INT_LEAST16_MIN: ::std::os::raw::c_int = -32768;
pub const INT_LEAST32_MIN: ::std::os::raw::c_int = -2147483648;
pub const INT_LEAST8_MAX: ::std::os::raw::c_uint = 127;
pub const INT_LEAST16_MAX: ::std::os::raw::c_uint = 32767;
pub const INT_LEAST32_MAX: ::std::os::raw::c_uint = 2147483647;
pub const UINT_LEAST8_MAX: ::std::os::raw::c_uint = 255;
pub const UINT_LEAST16_MAX: ::std::os::raw::c_uint = 65535;
pub const UINT_LEAST32_MAX: ::std::os::raw::c_uint = 4294967295;
pub const INT_FAST8_MIN: ::std::os::raw::c_int = -128;
pub const INT_FAST16_MIN: ::std::os::raw::c_longlong = -9223372036854775808;
pub const INT_FAST32_MIN: ::std::os::raw::c_longlong = -9223372036854775808;
pub const INT_FAST8_MAX: ::std::os::raw::c_uint = 127;
pub const INT_FAST16_MAX: ::std::os::raw::c_ulonglong = 9223372036854775807;
pub const INT_FAST32_MAX: ::std::os::raw::c_ulonglong = 9223372036854775807;
pub const UINT_FAST8_MAX: ::std::os::raw::c_uint = 255;
pub const UINT_FAST16_MAX: ::std::os::raw::c_int = -1;
pub const UINT_FAST32_MAX: ::std::os::raw::c_int = -1;
pub const INTPTR_MIN: ::std::os::raw::c_longlong = -9223372036854775808;
pub const INTPTR_MAX: ::std::os::raw::c_ulonglong = 9223372036854775807;
pub const UINTPTR_MAX: ::std::os::raw::c_int = -1;
pub const PTRDIFF_MIN: ::std::os::raw::c_longlong = -9223372036854775808;
pub const PTRDIFF_MAX: ::std::os::raw::c_ulonglong = 9223372036854775807;
pub const SIG_ATOMIC_MIN: ::std::os::raw::c_int = -2147483648;
pub const SIG_ATOMIC_MAX: ::std::os::raw::c_uint = 2147483647;
pub const SIZE_MAX: ::std::os::raw::c_int = -1;
pub const WINT_MIN: ::std::os::raw::c_uint = 0;
pub const WINT_MAX: ::std::os::raw::c_uint = 4294967295;
pub const __BITS_PER_LONG: ::std::os::raw::c_uint = 64;
pub const __FD_SETSIZE: ::std::os::raw::c_uint = 1024;
pub const VRING_DESC_F_NEXT: ::std::os::raw::c_uint = 1;
pub const VRING_DESC_F_WRITE: ::std::os::raw::c_uint = 2;
pub const VRING_DESC_F_INDIRECT: ::std::os::raw::c_uint = 4;
pub const VRING_USED_F_NO_NOTIFY: ::std::os::raw::c_uint = 1;
pub const VRING_AVAIL_F_NO_INTERRUPT: ::std::os::raw::c_uint = 1;
pub const VIRTIO_RING_F_INDIRECT_DESC: ::std::os::raw::c_uint = 28;
pub const VIRTIO_RING_F_EVENT_IDX: ::std::os::raw::c_uint = 29;
pub const VRING_AVAIL_ALIGN_SIZE: ::std::os::raw::c_uint = 2;
pub const VRING_USED_ALIGN_SIZE: ::std::os::raw::c_uint = 4;
pub const VRING_DESC_ALIGN_SIZE: ::std::os::raw::c_uint = 16;
pub type int_least8_t = ::std::os::raw::c_schar;
pub type int_least16_t = ::std::os::raw::c_short;
pub type int_least32_t = ::std::os::raw::c_int;
pub type int_least64_t = ::std::os::raw::c_long;
pub type uint_least8_t = ::std::os::raw::c_uchar;
pub type uint_least16_t = ::std::os::raw::c_ushort;
pub type uint_least32_t = ::std::os::raw::c_uint;
pub type uint_least64_t = ::std::os::raw::c_ulong;
pub type int_fast8_t = ::std::os::raw::c_schar;
pub type int_fast16_t = ::std::os::raw::c_long;
pub type int_fast32_t = ::std::os::raw::c_long;
pub type int_fast64_t = ::std::os::raw::c_long;
pub type uint_fast8_t = ::std::os::raw::c_uchar;
pub type uint_fast16_t = ::std::os::raw::c_ulong;
pub type uint_fast32_t = ::std::os::raw::c_ulong;
pub type uint_fast64_t = ::std::os::raw::c_ulong;
pub type intmax_t = ::std::os::raw::c_long;
pub type uintmax_t = ::std::os::raw::c_ulong;
pub type __s8 = ::std::os::raw::c_schar;
pub type __u8 = ::std::os::raw::c_uchar;
pub type __s16 = ::std::os::raw::c_short;
pub type __u16 = ::std::os::raw::c_ushort;
pub type __s32 = ::std::os::raw::c_int;
pub type __u32 = ::std::os::raw::c_uint;
pub type __s64 = ::std::os::raw::c_longlong;
pub type __u64 = ::std::os::raw::c_ulonglong;
#[repr(C)]
#[derive(Debug, Copy)]
pub struct __kernel_fd_set {
    pub fds_bits: [::std::os::raw::c_ulong; 16usize],
}
#[test]
fn bindgen_test_layout___kernel_fd_set() {
    assert_eq!(::std::mem::size_of::<__kernel_fd_set>() , 128usize , concat !
               ( "Size of: " , stringify ! ( __kernel_fd_set ) ));
    assert_eq! (::std::mem::align_of::<__kernel_fd_set>() , 8usize , concat !
                ( "Alignment of " , stringify ! ( __kernel_fd_set ) ));
    assert_eq! (unsafe {
                & ( * ( 0 as * const __kernel_fd_set ) ) . fds_bits as * const
                _ as usize } , 0usize , concat ! (
                "Alignment of field: " , stringify ! ( __kernel_fd_set ) ,
                "::" , stringify ! ( fds_bits ) ));
}
impl Clone for __kernel_fd_set {
    fn clone(&self) -> Self { *self }
}
pub type __kernel_sighandler_t =
    ::std::option::Option<unsafe extern "C" fn(arg1: ::std::os::raw::c_int)>;
pub type __kernel_key_t = ::std::os::raw::c_int;
pub type __kernel_mqd_t = ::std::os::raw::c_int;
pub type __kernel_old_uid_t = ::std::os::raw::c_ushort;
pub type __kernel_old_gid_t = ::std::os::raw::c_ushort;
pub type __kernel_old_dev_t = ::std::os::raw::c_ulong;
pub type __kernel_long_t = ::std::os::raw::c_long;
pub type __kernel_ulong_t = ::std::os::raw::c_ulong;
pub type __kernel_ino_t = __kernel_ulong_t;
pub type __kernel_mode_t = ::std::os::raw::c_uint;
pub type __kernel_pid_t = ::std::os::raw::c_int;
pub type __kernel_ipc_pid_t = ::std::os::raw::c_int;
pub type __kernel_uid_t = ::std::os::raw::c_uint;
pub type __kernel_gid_t = ::std::os::raw::c_uint;
pub type __kernel_suseconds_t = __kernel_long_t;
pub type __kernel_daddr_t = ::std::os::raw::c_int;
pub type __kernel_uid32_t = ::std::os::raw::c_uint;
pub type __kernel_gid32_t = ::std::os::raw::c_uint;
pub type __kernel_size_t = __kernel_ulong_t;
pub type __kernel_ssize_t = __kernel_long_t;
pub type __kernel_ptrdiff_t = __kernel_long_t;
#[repr(C)]
#[derive(Debug, Copy)]
pub struct __kernel_fsid_t {
    pub val: [::std::os::raw::c_int; 2usize],
}
#[test]
fn bindgen_test_layout___kernel_fsid_t() {
    assert_eq!(::std::mem::size_of::<__kernel_fsid_t>() , 8usize , concat ! (
               "Size of: " , stringify ! ( __kernel_fsid_t ) ));
    assert_eq! (::std::mem::align_of::<__kernel_fsid_t>() , 4usize , concat !
                ( "Alignment of " , stringify ! ( __kernel_fsid_t ) ));
    assert_eq! (unsafe {
                & ( * ( 0 as * const __kernel_fsid_t ) ) . val as * const _ as
                usize } , 0usize , concat ! (
                "Alignment of field: " , stringify ! ( __kernel_fsid_t ) ,
                "::" , stringify ! ( val ) ));
}
impl Clone for __kernel_fsid_t {
    fn clone(&self) -> Self { *self }
}
pub type __kernel_off_t = __kernel_long_t;
pub type __kernel_loff_t = ::std::os::raw::c_longlong;
pub type __kernel_time_t = __kernel_long_t;
pub type __kernel_clock_t = __kernel_long_t;
pub type __kernel_timer_t = ::std::os::raw::c_int;
pub type __kernel_clockid_t = ::std::os::raw::c_int;
pub type __kernel_caddr_t = *mut ::std::os::raw::c_char;
pub type __kernel_uid16_t = ::std::os::raw::c_ushort;
pub type __kernel_gid16_t = ::std::os::raw::c_ushort;
pub type __le16 = __u16;
pub type __be16 = __u16;
pub type __le32 = __u32;
pub type __be32 = __u32;
pub type __le64 = __u64;
pub type __be64 = __u64;
pub type __sum16 = __u16;
pub type __wsum = __u32;
pub type __virtio16 = __u16;
pub type __virtio32 = __u32;
pub type __virtio64 = __u64;
#[repr(C)]
#[derive(Debug, Copy)]
pub struct vring_desc {
    pub addr: __virtio64,
    pub len: __virtio32,
    pub flags: __virtio16,
    pub next: __virtio16,
}
#[test]
fn bindgen_test_layout_vring_desc() {
    assert_eq!(::std::mem::size_of::<vring_desc>() , 16usize , concat ! (
               "Size of: " , stringify ! ( vring_desc ) ));
    assert_eq! (::std::mem::align_of::<vring_desc>() , 8usize , concat ! (
                "Alignment of " , stringify ! ( vring_desc ) ));
    assert_eq! (unsafe {
                & ( * ( 0 as * const vring_desc ) ) . addr as * const _ as
                usize } , 0usize , concat ! (
                "Alignment of field: " , stringify ! ( vring_desc ) , "::" ,
                stringify ! ( addr ) ));
    assert_eq! (unsafe {
                & ( * ( 0 as * const vring_desc ) ) . len as * const _ as
                usize } , 8usize , concat ! (
                "Alignment of field: " , stringify ! ( vring_desc ) , "::" ,
                stringify ! ( len ) ));
    assert_eq! (unsafe {
                & ( * ( 0 as * const vring_desc ) ) . flags as * const _ as
                usize } , 12usize , concat ! (
                "Alignment of field: " , stringify ! ( vring_desc ) , "::" ,
                stringify ! ( flags ) ));
    assert_eq! (unsafe {
                & ( * ( 0 as * const vring_desc ) ) . next as * const _ as
                usize } , 14usize , concat ! (
                "Alignment of field: " , stringify ! ( vring_desc ) , "::" ,
                stringify ! ( next ) ));
}
impl Clone for vring_desc {
    fn clone(&self) -> Self { *self }
}
#[repr(C)]
#[derive(Debug, Copy)]
pub struct vring_avail {
    pub flags: __virtio16,
    pub idx: __virtio16,
    pub ring: __IncompleteArrayField<__virtio16>,
}
#[test]
fn bindgen_test_layout_vring_avail() {
    assert_eq!(::std::mem::size_of::<vring_avail>() , 4usize , concat ! (
               "Size of: " , stringify ! ( vring_avail ) ));
    assert_eq! (::std::mem::align_of::<vring_avail>() , 2usize , concat ! (
                "Alignment of " , stringify ! ( vring_avail ) ));
}
impl Clone for vring_avail {
    fn clone(&self) -> Self { *self }
}
#[repr(C)]
#[derive(Debug, Copy)]
pub struct vring_used_elem {
    pub id: __virtio32,
    pub len: __virtio32,
}
#[test]
fn bindgen_test_layout_vring_used_elem() {
    assert_eq!(::std::mem::size_of::<vring_used_elem>() , 8usize , concat ! (
               "Size of: " , stringify ! ( vring_used_elem ) ));
    assert_eq! (::std::mem::align_of::<vring_used_elem>() , 4usize , concat !
                ( "Alignment of " , stringify ! ( vring_used_elem ) ));
    assert_eq! (unsafe {
                & ( * ( 0 as * const vring_used_elem ) ) . id as * const _ as
                usize } , 0usize , concat ! (
                "Alignment of field: " , stringify ! ( vring_used_elem ) ,
                "::" , stringify ! ( id ) ));
    assert_eq! (unsafe {
                & ( * ( 0 as * const vring_used_elem ) ) . len as * const _ as
                usize } , 4usize , concat ! (
                "Alignment of field: " , stringify ! ( vring_used_elem ) ,
                "::" , stringify ! ( len ) ));
}
impl Clone for vring_used_elem {
    fn clone(&self) -> Self { *self }
}
#[repr(C)]
#[derive(Debug, Copy)]
pub struct vring_used {
    pub flags: __virtio16,
    pub idx: __virtio16,
    pub ring: __IncompleteArrayField<vring_used_elem>,
    pub __bindgen_align: [u32; 0usize],
}
#[test]
fn bindgen_test_layout_vring_used() {
    assert_eq!(::std::mem::size_of::<vring_used>() , 4usize , concat ! (
               "Size of: " , stringify ! ( vring_used ) ));
    assert_eq! (::std::mem::align_of::<vring_used>() , 4usize , concat ! (
                "Alignment of " , stringify ! ( vring_used ) ));
}
impl Clone for vring_used {
    fn clone(&self) -> Self { *self }
}
#[repr(C)]
#[derive(Debug, Copy)]
pub struct vring {
    pub num: ::std::os::raw::c_uint,
    pub desc: *mut vring_desc,
    pub avail: *mut vring_avail,
    pub used: *mut vring_used,
}
#[test]
fn bindgen_test_layout_vring() {
    assert_eq!(::std::mem::size_of::<vring>() , 32usize , concat ! (
               "Size of: " , stringify ! ( vring ) ));
    assert_eq! (::std::mem::align_of::<vring>() , 8usize , concat ! (
                "Alignment of " , stringify ! ( vring ) ));
    assert_eq! (unsafe {
                & ( * ( 0 as * const vring ) ) . num as * const _ as usize } ,
                0usize , concat ! (
                "Alignment of field: " , stringify ! ( vring ) , "::" ,
                stringify ! ( num ) ));
    assert_eq! (unsafe {
                & ( * ( 0 as * const vring ) ) . desc as * const _ as usize }
                , 8usize , concat ! (
                "Alignment of field: " , stringify ! ( vring ) , "::" ,
                stringify ! ( desc ) ));
    assert_eq! (unsafe {
                & ( * ( 0 as * const vring ) ) . avail as * const _ as usize }
                , 16usize , concat ! (
                "Alignment of field: " , stringify ! ( vring ) , "::" ,
                stringify ! ( avail ) ));
    assert_eq! (unsafe {
                & ( * ( 0 as * const vring ) ) . used as * const _ as usize }
                , 24usize , concat ! (
                "Alignment of field: " , stringify ! ( vring ) , "::" ,
                stringify ! ( used ) ));
}
impl Clone for vring {
    fn clone(&self) -> Self { *self }
}
