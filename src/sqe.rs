use std::io;
use std::mem;
use std::os::unix::io::RawFd;
use std::ptr::{self, NonNull};
use std::marker::PhantomData;

use super::{IoUring, sys};

const IORING_OP_NOP:                libc::__u8 = 0;
const IORING_OP_READV:              libc::__u8 = 1;
const IORING_OP_WRITEV:             libc::__u8 = 2;
const IORING_OP_FSYNC:              libc::__u8 = 3;
const IORING_OP_READ_FIXED:         libc::__u8 = 4;
const IORING_OP_WRITE_FIXED:        libc::__u8 = 5;

pub struct SubmissionQueue<'ring> {
    ring: NonNull<sys::io_uring>,
    _marker: PhantomData<&'ring mut IoUring>,
}

impl<'ring> SubmissionQueue<'ring> {
    pub(crate) fn new(ring: &'ring IoUring) -> SubmissionQueue<'ring> {
        SubmissionQueue {
            ring: NonNull::from(&ring.ring),
            _marker: PhantomData,
        }
    }

    pub fn next_sqe<'a>(&'a mut self) -> Option<SubmissionQueueEvent<'a>> {
        unsafe {
            let sqe = sys::io_uring_get_sqe(self.ring.as_ptr());
            if sqe != ptr::null_mut() {
                Some(SubmissionQueueEvent::new(&mut *sqe))
            } else {
                None
            }
        }
    }

    pub fn submit(&mut self) -> io::Result<usize> {
        let ret = unsafe { sys::io_uring_submit(self.ring.as_ptr()) };
        if ret >= 0 {
            Ok(ret as _)
        } else {
            Err(io::Error::from_raw_os_error(ret))
        }
    }

    pub fn submit_and_wait(&mut self, wait_for: u32) -> io::Result<usize> {
        let ret = unsafe { sys::io_uring_submit_and_wait(self.ring.as_ptr(), wait_for as _) };
        if ret >= 0 {
            Ok(ret as _)
        } else {
            Err(io::Error::from_raw_os_error(ret))
        }
    }
}

pub struct SubmissionQueueEvent<'a> {
    sqe: &'a mut sys::io_uring_sqe,
}

impl<'a> SubmissionQueueEvent<'a> {
    pub(crate) fn new(sqe: &'a mut sys::io_uring_sqe) -> SubmissionQueueEvent<'a> {
        SubmissionQueueEvent { sqe }
    }

    pub fn user_data(&self) -> u64 {
        self.sqe.user_data as u64
    }

    pub fn set_user_data(&mut self, user_data: u64) {
        self.sqe.user_data = user_data as _;
    }

    pub fn flags(&self) -> SubmissionFlags {
        unsafe { SubmissionFlags::from_bits_unchecked(self.sqe.flags as _) }
    }

    pub fn set_flags(&mut self, flags: SubmissionFlags) {
        self.sqe.flags = flags.bits() as _;
    }

    #[inline]
    pub unsafe fn prep_read_vectored(
        &mut self,
        fd: RawFd,
        bufs: &mut [io::IoSliceMut<'_>],
        offset: usize,
    ) {
        let len = bufs.len();
        let addr = bufs as *mut [io::IoSliceMut<'_>] as *mut libc::iovec;
        self.sqe.opcode = IORING_OP_READV;
        self.sqe.fd = fd;
        self.sqe.off_addr2.off = offset as _;
        self.sqe.addr = addr as _;
        self.sqe.len = len as _;
    }

    #[inline]
    pub unsafe fn prep_read_fixed(
        &mut self,
        fd: RawFd,
        buf: &mut [u8],
        offset: usize,
        buf_index: usize,
    ) {
        let len = buf.len();
        let addr = buf as *mut [u8] as *mut libc::c_void;
        self.sqe.opcode = IORING_OP_READ_FIXED;
        self.sqe.fd = fd;
        self.sqe.off_addr2.off = offset as _;
        self.sqe.addr = addr as _;
        self.sqe.len = len as _;
        self.sqe.buf_index.buf_index = buf_index as _;
        self.sqe.flags |= SubmissionFlags::FIXED_FILE.bits();
    }

    #[inline]
    pub unsafe fn prep_write_vectored(
        &mut self,
        fd: RawFd,
        bufs: &[io::IoSlice<'_>],
        offset: usize,
    ) {
        let len = bufs.len();
        let addr = bufs as *const [io::IoSlice<'_>] as *const libc::iovec;
        self.sqe.opcode = IORING_OP_WRITEV;
        self.sqe.fd = fd;
        self.sqe.off_addr2.off = offset as _;
        self.sqe.addr = addr as _;
        self.sqe.len = len as _;
    }

    #[inline]
    pub unsafe fn prep_write_fixed(
        &mut self,
        fd: RawFd,
        buf: &[u8],
        offset: usize,
        buf_index: usize,
    ) {
        let len = buf.len();
        let addr = buf as *const [u8] as *const libc::c_void;
        self.sqe.opcode = IORING_OP_WRITE_FIXED;
        self.sqe.fd = fd;
        self.sqe.off_addr2.off = offset as _;
        self.sqe.addr = addr as _;
        self.sqe.len = len as _;
        self.sqe.buf_index.buf_index = buf_index as _;
        self.sqe.flags |= SubmissionFlags::FIXED_FILE.bits();
    }

    #[inline]
    pub unsafe fn prep_fsync(&mut self, fd: RawFd, flags: FsyncFlags) {
        self.sqe.opcode = IORING_OP_FSYNC;
        self.sqe.fd = fd;
        self.sqe.off_addr2.off = 0;
        self.sqe.addr = 0;
        self.sqe.len = 0;
        self.sqe.cmd_flags.fsync_flags = flags.bits();
    }

    #[inline]
    pub unsafe fn prep_nop(&mut self) {
        self.sqe.opcode = IORING_OP_NOP;
        self.sqe.fd = 0;
        self.sqe.off_addr2.off = 0;
        self.sqe.addr = 0;
        self.sqe.len = 0;
    }

    pub fn clear(&mut self) {
        *self.sqe = unsafe { mem::zeroed() };
    }

    pub fn raw(&self) -> &sys::io_uring_sqe {
        &self.sqe
    }

    pub fn raw_mut(&mut self) -> &mut sys::io_uring_sqe {
        &mut self.sqe
    }
}

bitflags::bitflags! {
    pub struct SubmissionFlags: libc::__u8 {
        const FIXED_FILE    = 1 << 0;   /* use fixed fileset */
        const IO_DRAIN      = 1 << 1;   /* issue after inflight IO */
        const IO_LINK       = 1 << 2;   /* next IO depends on this one */
    }
}

bitflags::bitflags! {
    pub struct FsyncFlags: libc::c_uint {
        const FSYNC_DATASYNC    = 1 << 0;
    }
}
