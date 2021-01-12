// Copyright 2021 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use std::cell::RefCell;
use std::cmp::{max, min};
use std::convert::TryFrom;
use std::fmt::{self, Display};
use std::io::{self, Write};
use std::mem::size_of;
use std::rc::Rc;
use std::result;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
use std::u32;

use futures::pin_mut;
use futures::stream::{FuturesUnordered, StreamExt};
use libchromeos::sync::Mutex as AsyncMutex;

use base::Error as SysError;
use base::Result as SysResult;
use base::{error, info, iov_max, warn, AsRawDescriptor, Event, RawDescriptor, Timer};
use cros_async::{select2, AsyncError, EventAsync, TimerAsync};
use data_model::{DataInit, Le16, Le32, Le64};
use disk::{AsyncDisk, ToAsyncDisk};
use msg_socket::{MsgError, MsgSender};
use vm_control::{DiskControlCommand, DiskControlResponseSocket, DiskControlResult};
use vm_memory::GuestMemory;

use super::{
    copy_config, DescriptorChain, DescriptorError, Interrupt, Queue, Reader, SignalableInterrupt,
    VirtioDevice, Writer, TYPE_BLOCK,
};

const QUEUE_SIZE: u16 = 256;
const NUM_QUEUES: u16 = 16;
const QUEUE_SIZES: &[u16] = &[QUEUE_SIZE; NUM_QUEUES as usize];
const SECTOR_SHIFT: u8 = 9;
const SECTOR_SIZE: u64 = 0x01 << SECTOR_SHIFT;
const MAX_DISCARD_SECTORS: u32 = u32::MAX;
const MAX_WRITE_ZEROES_SECTORS: u32 = u32::MAX;
// Arbitrary limits for number of discard/write zeroes segments.
const MAX_DISCARD_SEG: u32 = 32;
const MAX_WRITE_ZEROES_SEG: u32 = 32;
// Hard-coded to 64 KiB (in 512-byte sectors) for now,
// but this should probably be based on cluster size for qcow.
const DISCARD_SECTOR_ALIGNMENT: u32 = 128;

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;
const VIRTIO_BLK_T_FLUSH: u32 = 4;
const VIRTIO_BLK_T_DISCARD: u32 = 11;
const VIRTIO_BLK_T_WRITE_ZEROES: u32 = 13;

const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;
const VIRTIO_BLK_S_UNSUPP: u8 = 2;

const VIRTIO_BLK_F_SEG_MAX: u32 = 2;
const VIRTIO_BLK_F_RO: u32 = 5;
const VIRTIO_BLK_F_BLK_SIZE: u32 = 6;
const VIRTIO_BLK_F_FLUSH: u32 = 9;
const VIRTIO_BLK_F_MQ: u32 = 12;
const VIRTIO_BLK_F_DISCARD: u32 = 13;
const VIRTIO_BLK_F_WRITE_ZEROES: u32 = 14;

#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
struct virtio_blk_geometry {
    cylinders: Le16,
    heads: u8,
    sectors: u8,
}

// Safe because it only has data and has no implicit padding.
unsafe impl DataInit for virtio_blk_geometry {}

#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
struct virtio_blk_topology {
    physical_block_exp: u8,
    alignment_offset: u8,
    min_io_size: Le16,
    opt_io_size: Le32,
}

// Safe because it only has data and has no implicit padding.
unsafe impl DataInit for virtio_blk_topology {}

#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
struct virtio_blk_config {
    capacity: Le64,
    size_max: Le32,
    seg_max: Le32,
    geometry: virtio_blk_geometry,
    blk_size: Le32,
    topology: virtio_blk_topology,
    writeback: u8,
    unused0: u8,
    num_queues: Le16,
    max_discard_sectors: Le32,
    max_discard_seg: Le32,
    discard_sector_alignment: Le32,
    max_write_zeroes_sectors: Le32,
    max_write_zeroes_seg: Le32,
    write_zeroes_may_unmap: u8,
    unused1: [u8; 3],
}

// Safe because it only has data and has no implicit padding.
unsafe impl DataInit for virtio_blk_req_header {}

#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
struct virtio_blk_req_header {
    req_type: Le32,
    reserved: Le32,
    sector: Le64,
}

// Safe because it only has data and has no implicit padding.
unsafe impl DataInit for virtio_blk_config {}

#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
struct virtio_blk_discard_write_zeroes {
    sector: Le64,
    num_sectors: Le32,
    flags: Le32,
}

const VIRTIO_BLK_DISCARD_WRITE_ZEROES_FLAG_UNMAP: u32 = 1 << 0;

// Safe because it only has data and has no implicit padding.
unsafe impl DataInit for virtio_blk_discard_write_zeroes {}

#[derive(Debug)]
enum ExecuteError {
    // Error creating a receiver for command messages.
    CreatingMessageReceiver(MsgError),
    Descriptor(DescriptorError),
    Read(io::Error),
    ReceivingCommand(MsgError),
    SendingResponse(MsgError),
    WriteStatus(io::Error),
    /// Error arming the flush timer.
    Flush(disk::Error),
    ReadIo {
        length: usize,
        sector: u64,
        desc_error: disk::Error,
    },
    WriteIo {
        length: usize,
        sector: u64,
        desc_error: disk::Error,
    },
    DiscardWriteZeroes {
        ioerr: Option<disk::Error>,
        sector: u64,
        num_sectors: u32,
        flags: u32,
    },
    ReadOnly {
        request_type: u32,
    },
    OutOfRange,
    MissingStatus,
    Unsupported(u32),
}

impl Display for ExecuteError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ExecuteError::*;

        match self {
            CreatingMessageReceiver(e) => write!(f, "couldn't create a message receiver: {}", e),
            Descriptor(e) => write!(f, "virtio descriptor error: {}", e),
            Read(e) => write!(f, "failed to read message: {}", e),
            ReceivingCommand(e) => write!(f, "failed to read command message: {}", e),
            SendingResponse(e) => write!(f, "failed to send command response: {}", e),
            WriteStatus(e) => write!(f, "failed to write request status: {}", e),
            Flush(e) => write!(f, "failed to flush: {}", e),
            ReadIo {
                length,
                sector,
                desc_error,
            } => write!(
                f,
                "io error reading {} bytes from sector {}: {}",
                length, sector, desc_error,
            ),
            WriteIo {
                length,
                sector,
                desc_error,
            } => write!(
                f,
                "io error writing {} bytes to sector {}: {}",
                length, sector, desc_error,
            ),
            DiscardWriteZeroes {
                ioerr: Some(ioerr),
                sector,
                num_sectors,
                flags,
            } => write!(
                f,
                "failed to perform discard or write zeroes; sector={} num_sectors={} flags={}; {}",
                sector, num_sectors, flags, ioerr,
            ),
            DiscardWriteZeroes {
                ioerr: None,
                sector,
                num_sectors,
                flags,
            } => write!(
                f,
                "failed to perform discard or write zeroes; sector={} num_sectors={} flags={}",
                sector, num_sectors, flags,
            ),
            ReadOnly { request_type } => write!(f, "read only; request_type={}", request_type),
            OutOfRange => write!(f, "out of range"),
            MissingStatus => write!(f, "not enough space in descriptor chain to write status"),
            Unsupported(n) => write!(f, "unsupported ({})", n),
        }
    }
}

impl ExecuteError {
    fn status(&self) -> u8 {
        match self {
            ExecuteError::CreatingMessageReceiver(_) => VIRTIO_BLK_S_IOERR,
            ExecuteError::Descriptor(_) => VIRTIO_BLK_S_IOERR,
            ExecuteError::Read(_) => VIRTIO_BLK_S_IOERR,
            ExecuteError::ReceivingCommand(_) => VIRTIO_BLK_S_IOERR,
            ExecuteError::SendingResponse(_) => VIRTIO_BLK_S_IOERR,
            ExecuteError::WriteStatus(_) => VIRTIO_BLK_S_IOERR,
            ExecuteError::Flush(_) => VIRTIO_BLK_S_IOERR,
            ExecuteError::ReadIo { .. } => VIRTIO_BLK_S_IOERR,
            ExecuteError::WriteIo { .. } => VIRTIO_BLK_S_IOERR,
            ExecuteError::DiscardWriteZeroes { .. } => VIRTIO_BLK_S_IOERR,
            ExecuteError::ReadOnly { .. } => VIRTIO_BLK_S_IOERR,
            ExecuteError::OutOfRange { .. } => VIRTIO_BLK_S_IOERR,
            ExecuteError::MissingStatus => VIRTIO_BLK_S_IOERR,
            ExecuteError::Unsupported(_) => VIRTIO_BLK_S_UNSUPP,
        }
    }
}

// Errors that happen in block outside of executing a request.
#[derive(Debug)]
enum OtherError {
    AsyncKillCreate(AsyncError),
    AsyncResampleCreate(AsyncError),
    AsyncTimerCreate(AsyncError),
    CloneResampleEvent(base::Error),
    FsyncDisk(disk::Error),
    ReadResampleEvent(AsyncError),
    TimerCreate(base::Error),
    TimerReset(base::Error),
}

impl Display for OtherError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::OtherError::*;

        match self {
            AsyncKillCreate(e) => write!(f, "couldn't create an async kill event fd: {}", e),
            AsyncTimerCreate(e) => write!(f, "couldn't create an async timer: {}", e),
            AsyncResampleCreate(e) => write!(f, "couldn't create an async resample event: {}", e),
            CloneResampleEvent(e) => write!(f, "couldn't clone the resample event: {}", e),
            FsyncDisk(e) => write!(f, "failed to fsync the disk: {}", e),
            ReadResampleEvent(e) => write!(f, "couldn't read the resample event: {}", e),
            TimerCreate(e) => write!(f, "couldn't create a timer: {}", e),
            TimerReset(e) => write!(f, "couldn't reset the timer: {}", e),
        }
    }
}

struct DiskState {
    disk_image: Box<dyn AsyncDisk>,
    // `disk_size` is shared between async contexts requiring the mutex, however the main thread can
    // also read it at any time to fill config register requests which means it must also be
    // atomically updated.
    disk_size: AsyncMutex<Arc<AtomicU64>>,
    read_only: bool,
    sparse: bool,
}

async fn process_one_request(
    avail_desc: DescriptorChain,
    disk_state: Rc<RefCell<DiskState>>,
    flush_timer_armed: Rc<AtomicBool>,
    mem: &GuestMemory,
) -> result::Result<usize, ExecuteError> {
    let mut reader =
        Reader::new(mem.clone(), avail_desc.clone()).map_err(ExecuteError::Descriptor)?;
    let mut writer = Writer::new(mem.clone(), avail_desc).map_err(ExecuteError::Descriptor)?;

    // The last byte of the buffer is virtio_blk_req::status.
    // Split it into a separate Writer so that status_writer is the final byte and
    // the original writer is left with just the actual block I/O data.
    let available_bytes = writer.available_bytes();
    let status_offset = available_bytes
        .checked_sub(1)
        .ok_or(ExecuteError::MissingStatus)?;
    let mut status_writer = writer.split_at(status_offset);

    let status =
        match BlockAsync::execute_request(&mut reader, &mut writer, disk_state, flush_timer_armed)
            .await
        {
            Ok(()) => VIRTIO_BLK_S_OK,
            Err(e) => {
                error!("failed executing disk request: {}", e);
                e.status()
            }
        };

    status_writer
        .write_all(&[status])
        .map_err(ExecuteError::WriteStatus)?;
    Ok(available_bytes)
}

async fn process_one_request_task(
    queue: Rc<RefCell<Queue>>,
    avail_desc: DescriptorChain,
    disk_state: Rc<RefCell<DiskState>>,
    mem: GuestMemory,
    interrupt: Rc<RefCell<Box<dyn SignalableInterrupt + Send>>>,
    flush_timer_armed: Rc<AtomicBool>,
) {
    let descriptor_index = avail_desc.index;

    let len = match process_one_request(avail_desc, Rc::clone(&disk_state), flush_timer_armed, &mem)
        .await
    {
        Ok(len) => len,
        Err(e) => {
            error!("block: failed to handle request: {}", e);
            0
        }
    };

    let mut queue = queue.borrow_mut();
    queue.add_used(&mem, descriptor_index, len as u32);
    //let int: &dyn SignalableInterrupt +'static= &*interrupt.borrow();
    queue.trigger_interrupt(&mem, (&*interrupt.borrow()).as_ref());
    queue.update_int_required(&mem);
}

// There is one async task running `handle_queue` per virtio queue in use.
// Receives messages from the guest and queues a task to complete the operations with the async
// executor.
async fn handle_queue(
    mem: &GuestMemory,
    disk_state: Rc<RefCell<DiskState>>,
    queue: Rc<RefCell<Queue>>,
    evt: EventAsync,
    interrupt: Box<dyn SignalableInterrupt + Send>,
    flush_timer_armed: Rc<AtomicBool>,
) {
    println!("handle queue start");
    let interrupt = Rc::new(RefCell::new(interrupt));
    loop {
        let descriptor_chain = queue.borrow_mut().pop(&mem);
        let descriptor_chain = match descriptor_chain {
            None => match evt.next_val().await {
                Err(e) => {
                    error!("Failed to read the next queue event: {}", e);
                    continue;
                }
                Ok(_) => continue,
            },
            Some(d) => d,
        };
        println!("got a kick");

        if let Err(e) = cros_async::add_future(Box::pin(process_one_request_task(
            Rc::clone(&queue),
            descriptor_chain,
            Rc::clone(&disk_state),
            mem.clone(),
            Rc::clone(&interrupt),
            Rc::clone(&flush_timer_armed),
        ))) {
            error!("Failed to queue disk request, aborting block: {}", e);
            return; // The inability to run tasks can't be recovered from.
        }
    }
}

async fn flush_timer_task(disk_state: Rc<RefCell<DiskState>>, timer_armed: Rc<AtomicBool>) {
    if let Err(e) = flush_timer(disk_state).await {
        error!("Failed to flush the disk: {}", e);
    }

    timer_armed.store(false, Ordering::Relaxed);
}

async fn flush_timer(disk_state: Rc<RefCell<DiskState>>) -> result::Result<(), OtherError> {
    // Delay after a write when the file is auto-flushed.
    let flush_delay = Duration::from_secs(60);

    let mut timer = Timer::new().map_err(OtherError::TimerCreate)?;
    timer
        .reset(flush_delay, None)
        .map_err(OtherError::TimerReset)?;
    let async_timer = TimerAsync::try_from(timer.0).map_err(OtherError::AsyncTimerCreate)?;

    let _ = async_timer.next_val().await;

    disk_state
        .borrow()
        .disk_image
        .fsync()
        .await
        .map_err(OtherError::FsyncDisk)
}

/*
async fn handle_irq_resample(interrupt: Rc<RefCell<Interrupt>>) -> result::Result<(), OtherError> {
    let resample_evt = interrupt
        .borrow_mut()
        .get_resample_evt()
        .try_clone()
        .map_err(OtherError::CloneResampleEvent)?;
    let resample_evt =
        EventAsync::try_from(resample_evt.0).map_err(OtherError::AsyncResampleCreate)?;
    loop {
        let _ = resample_evt
            .next_val()
            .await
            .map_err(OtherError::ReadResampleEvent)?;
        interrupt.borrow_mut().do_interrupt_resample();
    }
}
*/

async fn wait_kill(kill_evt: Event) -> result::Result<(), OtherError> {
    let kill_evt = EventAsync::try_from(kill_evt.0).map_err(OtherError::AsyncKillCreate)?;
    // Once this event is readable, exit. Exiting this future will cause the main loop to
    // break and the device process to exit.
    let _ = kill_evt.next_val().await;
    Ok(())
}
/*
async fn handle_command_socket(
    command_socket: &Option<DiskControlResponseSocket>,
    interrupts: Rc<RefCell<Vec<Interrupt>>>,
    disk_state: Rc<RefCell<DiskState>>,
) -> Result<(), ExecuteError> {
    let command_socket = match command_socket {
        Some(c) => c,
        None => {
            let () = futures::future::pending().await;
            return Ok(());
        }
    };
    let mut async_messages = command_socket
        .async_receiver()
        .map_err(ExecuteError::CreatingMessageReceiver)?;
    loop {
        match async_messages.next().await {
            Ok(command) => {
                let resp = match command {
                    DiskControlCommand::Resize { new_size } => {
                        resize(&mut disk_state.borrow_mut(), new_size).await
                    }
                };

                command_socket
                    .send(&resp)
                    .map_err(ExecuteError::SendingResponse)?;
                if let DiskControlResult::Ok = resp {
                    // TODO(dgreid) - This doesnt' make sense, config should be handled in the VMM.
                    let ints = interrupts.borrow_mut();
                    interrupts[0].signal_config_changed();
                }
            }
            Err(e) => return Err(ExecuteError::ReceivingCommand(e)),
        }
    }
}

async fn resize(disk_state: &mut DiskState, new_size: u64) -> DiskControlResult {
    if disk_state.read_only {
        error!("Attempted to resize read-only block device");
        return DiskControlResult::Err(SysError::new(libc::EROFS));
    }

    info!("Resizing block device to {} bytes", new_size);

    if let Err(e) = disk_state.disk_image.set_len(new_size) {
        error!("Resizing disk failed! {}", e);
        return DiskControlResult::Err(SysError::new(libc::EIO));
    }

    // Allocate new space if the disk image is not sparse.
    if let Err(e) = disk_state.disk_image.allocate(0, new_size) {
        error!("Allocating disk space after resize failed! {}", e);
        return DiskControlResult::Err(SysError::new(libc::EIO));
    }

    disk_state.sparse = false;

    if let Ok(new_disk_size) = disk_state.disk_image.get_len() {
        let disk_size = disk_state.disk_size.lock().await;
        disk_size.store(new_disk_size, Ordering::Release);
    }
    DiskControlResult::Ok
}
*/

fn run_worker(
    interrupts: Vec<Box<dyn SignalableInterrupt + Send>>,
    queues: Vec<Queue>,
    mem: GuestMemory,
    disk_state: &Rc<RefCell<DiskState>>,
    control_socket: &Option<DiskControlResponseSocket>,
    queue_evts: Vec<Event>,
    kill_evt: Event,
) -> Result<(), String> {
    // One flush timer per disk.
    let flush_timer_armed = Rc::new(AtomicBool::new(false));

    // Handle all the queues in one sub-select call.
    println!(
        "run worker spawn queues {} {} {}",
        queues.len(),
        queue_evts.len(),
        interrupts.len()
    );
    let queue_handlers =
        queues
            .into_iter()
            .filter(|q| q.ready)
            .map(|q| Rc::new(RefCell::new(q)))
            .zip(queue_evts.into_iter().map(|e| {
                EventAsync::try_from(e.0).expect("Failed to create async event for queue")
            }))
            .zip(interrupts.into_iter())
            .map(|((queue, event), interrupt)| {
                // alias some refs so the lifetimes work.
                let mem = &mem;
                let disk_state = &disk_state;
                println!("create handle queue box");
                Box::pin(handle_queue(
                    mem,
                    Rc::clone(disk_state),
                    Rc::clone(&queue),
                    event,
                    interrupt,
                    Rc::clone(&flush_timer_armed),
                ))
            })
            .collect::<FuturesUnordered<_>>()
            .into_future();

    //TODO(dgreid) how to deal with resize?
    // Handles control requests.
    //let control = handle_command_socket(control_socket, Rc::clone(&interrupts), disk_state.clone());
    //pin_mut!(control);

    // Process any requests to resample the irq value.
    //TODO -not needed for vhost user let resample = handle_irq_resample(Rc::clone(interrupts));
    //pin_mut!(resample);
    // Exit if the kill event is triggered.
    let kill = wait_kill(kill_evt);
    pin_mut!(kill);

    // And return once any future exits.
    println!("wait select2");
    let _ = select2(queue_handlers, /*control, resample,*/ kill);

    Ok(())
}

/// Virtio device for exposing block level read/write operations on a host file.
pub struct BlockAsync {
    kill_evt: Option<Event>,
    worker_thread:
        Option<thread::JoinHandle<(Box<dyn ToAsyncDisk>, Option<DiskControlResponseSocket>)>>,
    disk_image: Option<Box<dyn ToAsyncDisk>>,
    disk_size: Arc<AtomicU64>,
    avail_features: u64,
    read_only: bool,
    sparse: bool,
    seg_max: u32,
    block_size: u32,
    control_socket: Option<DiskControlResponseSocket>,
}

fn build_config_space(disk_size: u64, seg_max: u32, block_size: u32) -> virtio_blk_config {
    virtio_blk_config {
        // If the image is not a multiple of the sector size, the tail bits are not exposed.
        capacity: Le64::from(disk_size >> SECTOR_SHIFT),
        seg_max: Le32::from(seg_max),
        blk_size: Le32::from(block_size),
        num_queues: Le16::from(NUM_QUEUES),
        max_discard_sectors: Le32::from(MAX_DISCARD_SECTORS),
        discard_sector_alignment: Le32::from(DISCARD_SECTOR_ALIGNMENT),
        max_write_zeroes_sectors: Le32::from(MAX_WRITE_ZEROES_SECTORS),
        write_zeroes_may_unmap: 1,
        max_discard_seg: Le32::from(MAX_DISCARD_SEG),
        max_write_zeroes_seg: Le32::from(MAX_WRITE_ZEROES_SEG),
        ..Default::default()
    }
}

impl BlockAsync {
    /// Create a new virtio block device that operates on the given AsyncDisk.
    pub fn new(
        base_features: u64,
        disk_image: Box<dyn ToAsyncDisk>,
        read_only: bool,
        sparse: bool,
        block_size: u32,
        control_socket: Option<DiskControlResponseSocket>,
    ) -> SysResult<BlockAsync> {
        if block_size % SECTOR_SIZE as u32 != 0 {
            error!(
                "Block size {} is not a multiple of {}.",
                block_size, SECTOR_SIZE,
            );
            return Err(SysError::new(libc::EINVAL));
        }
        let disk_size = disk_image.get_len()?;
        if disk_size % block_size as u64 != 0 {
            warn!(
                "Disk size {} is not a multiple of block size {}; \
                 the remainder will not be visible to the guest.",
                disk_size, block_size,
            );
        }

        let mut avail_features: u64 = base_features;
        avail_features |= 1 << VIRTIO_BLK_F_FLUSH;
        if read_only {
            avail_features |= 1 << VIRTIO_BLK_F_RO;
        } else {
            if sparse {
                avail_features |= 1 << VIRTIO_BLK_F_DISCARD;
            }
            avail_features |= 1 << VIRTIO_BLK_F_WRITE_ZEROES;
        }
        avail_features |= 1 << VIRTIO_BLK_F_SEG_MAX;
        avail_features |= 1 << VIRTIO_BLK_F_BLK_SIZE;
        avail_features |= 1 << VIRTIO_BLK_F_MQ;

        let seg_max = min(max(iov_max(), 1), u32::max_value() as usize) as u32;

        // Since we do not currently support indirect descriptors, the maximum
        // number of segments must be smaller than the queue size.
        // In addition, the request header and status each consume a descriptor.
        let seg_max = min(seg_max, u32::from(QUEUE_SIZE) - 2);

        Ok(BlockAsync {
            kill_evt: None,
            worker_thread: None,
            disk_image: Some(disk_image),
            disk_size: Arc::new(AtomicU64::new(disk_size)),
            avail_features,
            read_only,
            sparse,
            seg_max,
            block_size,
            control_socket,
        })
    }

    // Execute a single block device request.
    // `writer` includes the data region only; the status byte is not included.
    // It is up to the caller to convert the result of this function into a status byte
    // and write it to the expected location in guest memory.
    async fn execute_request(
        reader: &mut Reader,
        writer: &mut Writer,
        disk_state: Rc<RefCell<DiskState>>,
        flush_timer_armed: Rc<AtomicBool>,
    ) -> result::Result<(), ExecuteError> {
        let req_header: virtio_blk_req_header = reader.read_obj().map_err(ExecuteError::Read)?;

        let req_type = req_header.req_type.to_native();
        let sector = req_header.sector.to_native();

        if disk_state.borrow().read_only && req_type != VIRTIO_BLK_T_IN {
            return Err(ExecuteError::ReadOnly {
                request_type: req_type,
            });
        }

        /// Check that a request accesses only data within the disk's current size.
        /// All parameters are in units of bytes.
        fn check_range(
            io_start: u64,
            io_length: u64,
            disk_size: u64,
        ) -> result::Result<(), ExecuteError> {
            let io_end = io_start
                .checked_add(io_length)
                .ok_or(ExecuteError::OutOfRange)?;
            if io_end > disk_size {
                Err(ExecuteError::OutOfRange)
            } else {
                Ok(())
            }
        }

        let disk_image = &disk_state.borrow().disk_image;

        // Take a read lock for the duration of this op so that the disk size doesn't change.
        let disk = disk_state.borrow();
        let disk_size_lock = disk.disk_size.read_lock().await;
        let disk_size = disk_size_lock.load(Ordering::Relaxed);

        match req_type {
            VIRTIO_BLK_T_IN => {
                let data_len = writer.available_bytes();
                if data_len == 0 {
                    return Ok(());
                }
                let offset = sector
                    .checked_shl(u32::from(SECTOR_SHIFT))
                    .ok_or(ExecuteError::OutOfRange)?;
                check_range(offset, data_len as u64, disk_size)?;
                writer
                    .write_all_from_at_fut(&**disk_image, data_len, offset)
                    .await
                    .map_err(|desc_error| ExecuteError::ReadIo {
                        length: data_len,
                        sector,
                        desc_error,
                    })?;
            }
            VIRTIO_BLK_T_OUT => {
                let data_len = reader.available_bytes();
                if data_len == 0 {
                    return Ok(());
                }
                let offset = sector
                    .checked_shl(u32::from(SECTOR_SHIFT))
                    .ok_or(ExecuteError::OutOfRange)?;
                check_range(offset, data_len as u64, disk_size)?;
                reader
                    .read_exact_to_at_fut(&**disk_image, data_len, offset)
                    .await
                    .map_err(|desc_error| ExecuteError::WriteIo {
                        length: data_len,
                        sector,
                        desc_error,
                    })?;
                // Ignore send errors, they aren't the client's fault and aren't fatal to disk ops.
                if !flush_timer_armed.load(Ordering::Relaxed) {
                    if cros_async::add_future(Box::pin(flush_timer_task(
                        Rc::clone(&disk_state),
                        Rc::clone(&flush_timer_armed),
                    )))
                    .is_ok()
                    {
                        flush_timer_armed.store(true, Ordering::Relaxed);
                    }
                }
            }
            VIRTIO_BLK_T_DISCARD | VIRTIO_BLK_T_WRITE_ZEROES => {
                if req_type == VIRTIO_BLK_T_DISCARD && !disk_state.borrow().sparse {
                    // Discard is a hint; if this is a non-sparse disk, just ignore it.
                    return Ok(());
                }

                while reader.available_bytes() >= size_of::<virtio_blk_discard_write_zeroes>() {
                    let seg: virtio_blk_discard_write_zeroes =
                        reader.read_obj().map_err(ExecuteError::Read)?;

                    let sector = seg.sector.to_native();
                    let num_sectors = seg.num_sectors.to_native();
                    let flags = seg.flags.to_native();

                    let valid_flags = if req_type == VIRTIO_BLK_T_WRITE_ZEROES {
                        VIRTIO_BLK_DISCARD_WRITE_ZEROES_FLAG_UNMAP
                    } else {
                        0
                    };

                    if (flags & !valid_flags) != 0 {
                        return Err(ExecuteError::DiscardWriteZeroes {
                            ioerr: None,
                            sector,
                            num_sectors,
                            flags,
                        });
                    }

                    let offset = sector
                        .checked_shl(u32::from(SECTOR_SHIFT))
                        .ok_or(ExecuteError::OutOfRange)?;
                    let length = u64::from(num_sectors)
                        .checked_shl(u32::from(SECTOR_SHIFT))
                        .ok_or(ExecuteError::OutOfRange)?;
                    check_range(offset, length, disk_size)?;

                    if req_type == VIRTIO_BLK_T_DISCARD {
                        // Since Discard is just a hint and some filesystems may not implement
                        // FALLOC_FL_PUNCH_HOLE, ignore punch_hole errors.
                        let _ = disk_image.punch_hole(offset, length).await;
                    } else {
                        disk_image
                            .write_zeroes_at(offset, length)
                            .await
                            .map_err(|e| ExecuteError::DiscardWriteZeroes {
                                ioerr: Some(e),
                                sector,
                                num_sectors,
                                flags,
                            })?;
                    }
                }
            }
            VIRTIO_BLK_T_FLUSH => {
                disk_image.fsync().await.map_err(ExecuteError::Flush)?;
            }
            t => return Err(ExecuteError::Unsupported(t)),
        };
        Ok(())
    }
}

impl Drop for BlockAsync {
    fn drop(&mut self) {
        if let Some(kill_evt) = self.kill_evt.take() {
            // Ignore the result because there is nothing we can do about it.
            let _ = kill_evt.write(1);
        }

        if let Some(worker_thread) = self.worker_thread.take() {
            let _ = worker_thread.join();
        }
    }
}

impl VirtioDevice for BlockAsync {
    fn keep_rds(&self) -> Vec<RawDescriptor> {
        let mut keep_rds = Vec::new();

        if let Some(disk_image) = &self.disk_image {
            keep_rds.extend(disk_image.as_raw_descriptors());
        }

        if let Some(control_socket) = &self.control_socket {
            keep_rds.push(control_socket.as_raw_descriptor());
        }

        keep_rds
    }

    fn features(&self) -> u64 {
        self.avail_features
    }

    fn device_type(&self) -> u32 {
        TYPE_BLOCK
    }

    fn queue_max_sizes(&self) -> &[u16] {
        QUEUE_SIZES
    }

    fn read_config(&self, offset: u64, data: &mut [u8]) {
        let config_space = {
            let disk_size = self.disk_size.load(Ordering::Acquire);
            build_config_space(disk_size, self.seg_max, self.block_size)
        };
        copy_config(data, 0, config_space.as_slice(), offset);
    }

    fn activate(
        &mut self,
        mem: GuestMemory,
        interrupt: Interrupt,
        queues: Vec<Queue>,
        queue_evts: Vec<Event>,
    ) {
    } // TODO(dgreid)

    fn activate_vhost(
        &mut self,
        mem: GuestMemory,
        interrupts: Vec<Box<dyn SignalableInterrupt + Send>>,
        queues: Vec<Queue>,
        queue_evts: Vec<Event>,
    ) {
        println!("activate vhost");
        let (self_kill_evt, kill_evt) = match Event::new().and_then(|e| Ok((e.try_clone()?, e))) {
            Ok(v) => v,
            Err(e) => {
                error!("failed creating kill Event pair: {}", e);
                return;
            }
        };
        self.kill_evt = Some(self_kill_evt);

        let read_only = self.read_only;
        let sparse = self.sparse;
        let disk_size = self.disk_size.clone();
        println!("activate vhost check disk");
        if let Some(disk_image) = self.disk_image.take() {
            println!("activate vhost has disk");
            let control_socket = self.control_socket.take();
            let worker_result =
                thread::Builder::new()
                    .name("virtio_blk".to_string())
                    .spawn(move || {
                        let async_image = match disk_image.to_async_disk() {
                            Ok(d) => d,
                            Err(e) => panic!("Failed to create async disk {}", e),
                        };
                        let disk_state = Rc::new(RefCell::new(DiskState {
                            disk_image: async_image,
                            disk_size: AsyncMutex::new(disk_size),
                            read_only,
                            sparse,
                        }));
                        if let Err(err_string) = run_worker(
                            interrupts,
                            queues,
                            mem,
                            &disk_state,
                            &control_socket,
                            queue_evts,
                            kill_evt,
                        ) {
                            error!("{}", err_string);
                        }

                        let disk_state = match Rc::try_unwrap(disk_state) {
                            Ok(d) => d.into_inner(),
                            Err(_) => panic!("too many refs to the disk"),
                        };
                        (disk_state.disk_image.into_inner(), control_socket)
                    });

            match worker_result {
                Err(e) => {
                    error!("failed to spawn virtio_blk worker: {}", e);
                    return;
                }
                Ok(join_handle) => {
                    self.worker_thread = Some(join_handle);
                }
            }
        }
    }

    fn reset(&mut self) -> bool {
        if let Some(kill_evt) = self.kill_evt.take() {
            if kill_evt.write(1).is_err() {
                error!("{}: failed to notify the kill event", self.debug_label());
                return false;
            }
        }

        if let Some(worker_thread) = self.worker_thread.take() {
            match worker_thread.join() {
                Err(_) => {
                    error!("{}: failed to get back resources", self.debug_label());
                    return false;
                }
                Ok((disk_image, control_socket)) => {
                    self.disk_image = Some(disk_image);
                    self.control_socket = control_socket;
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;
    use std::fs::{File, OpenOptions};
    use std::mem::size_of_val;

    use disk::SingleFileDisk;
    use tempfile::TempDir;
    use vm_memory::GuestAddress;

    use crate::virtio::base_features;
    use crate::virtio::descriptor_utils::{create_descriptor_chain, DescriptorType};

    use super::*;

    #[test]
    fn read_size() {
        let tempdir = TempDir::new().unwrap();
        let mut path = tempdir.path().to_owned();
        path.push("disk_image");
        let f = File::create(&path).unwrap();
        f.set_len(0x1000).unwrap();

        let features = base_features(false);
        let b = BlockAsync::new(features, Box::new(f), true, false, 512, None).unwrap();
        let mut num_sectors = [0u8; 4];
        b.read_config(0, &mut num_sectors);
        // size is 0x1000, so num_sectors is 8 (4096/512).
        assert_eq!([0x08, 0x00, 0x00, 0x00], num_sectors);
        let mut msw_sectors = [0u8; 4];
        b.read_config(4, &mut msw_sectors);
        // size is 0x1000, so msw_sectors is 0.
        assert_eq!([0x00, 0x00, 0x00, 0x00], msw_sectors);
    }

    #[test]
    fn read_block_size() {
        let tempdir = TempDir::new().unwrap();
        let mut path = tempdir.path().to_owned();
        path.push("disk_image");
        let f = File::create(&path).unwrap();
        f.set_len(0x1000).unwrap();

        let features = base_features(false);
        let b = BlockAsync::new(features, Box::new(f), true, false, 4096, None).unwrap();
        let mut blk_size = [0u8; 4];
        b.read_config(20, &mut blk_size);
        // blk_size should be 4096 (0x1000).
        assert_eq!([0x00, 0x10, 0x00, 0x00], blk_size);
    }

    #[test]
    fn read_features() {
        let tempdir = TempDir::new().unwrap();
        let mut path = tempdir.path().to_owned();
        path.push("disk_image");

        // read-write block device
        {
            let f = File::create(&path).unwrap();
            let features = base_features(false);
            let b = BlockAsync::new(features, Box::new(f), false, true, 512, None).unwrap();
            // writable device should set VIRTIO_BLK_F_FLUSH + VIRTIO_BLK_F_DISCARD
            // + VIRTIO_BLK_F_WRITE_ZEROES + VIRTIO_F_VERSION_1 + VIRTIO_BLK_F_BLK_SIZE
            // + VIRTIO_BLK_F_SEG_MAX + VIRTIO_BLK_F_MQ
            assert_eq!(0x100007244, b.features());
        }

        // read-write block device, non-sparse
        {
            let f = File::create(&path).unwrap();
            let features = base_features(false);
            let b = BlockAsync::new(features, Box::new(f), false, false, 512, None).unwrap();
            // read-only device should set VIRTIO_BLK_F_FLUSH and VIRTIO_BLK_F_RO
            // + VIRTIO_F_VERSION_1 + VIRTIO_BLK_F_BLK_SIZE + VIRTIO_BLK_F_SEG_MAX
            // + VIRTIO_BLK_F_MQ
            assert_eq!(0x100005244, b.features());
        }

        // read-only block device
        {
            let f = File::create(&path).unwrap();
            let features = base_features(false);
            let b = BlockAsync::new(features, Box::new(f), true, true, 512, None).unwrap();
            // read-only device should set VIRTIO_BLK_F_FLUSH and VIRTIO_BLK_F_RO
            // + VIRTIO_F_VERSION_1 + VIRTIO_BLK_F_BLK_SIZE + VIRTIO_BLK_F_SEG_MAX
            // + VIRTIO_BLK_F_MQ
            assert_eq!(0x100001264, b.features());
        }
    }

    #[test]
    fn read_last_sector() {
        let tempdir = TempDir::new().unwrap();
        let mut path = tempdir.path().to_owned();
        path.push("disk_image");
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .unwrap();
        let disk_size = 0x1000;
        f.set_len(disk_size).unwrap();
        let af = SingleFileDisk::try_from(f).expect("Failed to create SFD");

        let mem = Rc::new(
            GuestMemory::new(&[(GuestAddress(0u64), 4 * 1024 * 1024)])
                .expect("Creating guest memory failed."),
        );

        let req_hdr = virtio_blk_req_header {
            req_type: Le32::from(VIRTIO_BLK_T_IN),
            reserved: Le32::from(0),
            sector: Le64::from(7), // Disk is 8 sectors long, so this is the last valid sector.
        };
        mem.write_obj_at_addr(req_hdr, GuestAddress(0x1000))
            .expect("writing req failed");

        let avail_desc = create_descriptor_chain(
            &mem,
            GuestAddress(0x100),  // Place descriptor chain at 0x100.
            GuestAddress(0x1000), // Describe buffer at 0x1000.
            vec![
                // Request header
                (DescriptorType::Readable, size_of_val(&req_hdr) as u32),
                // I/O buffer (1 sector of data)
                (DescriptorType::Writable, 512),
                // Request status
                (DescriptorType::Writable, 1),
            ],
            0,
        )
        .expect("create_descriptor_chain failed");

        let flush_timer_armed = Rc::new(AtomicBool::new(false));

        let disk_state = Rc::new(RefCell::new(DiskState {
            disk_image: Box::new(af),
            disk_size: AsyncMutex::new(Arc::new(AtomicU64::new(disk_size))),
            read_only: false,
            sparse: true,
        }));

        let fut = process_one_request(avail_desc, disk_state, Rc::clone(&flush_timer_armed), &mem);

        cros_async::run_one(Box::pin(fut))
            .expect("running executor failed")
            .expect("execute failed");

        let status_offset = GuestAddress((0x1000 + size_of_val(&req_hdr) + 512) as u64);
        let status = mem.read_obj_from_addr::<u8>(status_offset).unwrap();
        assert_eq!(status, VIRTIO_BLK_S_OK);
    }

    #[test]
    fn read_beyond_last_sector() {
        let tempdir = TempDir::new().unwrap();
        let mut path = tempdir.path().to_owned();
        path.push("disk_image");
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .unwrap();
        let disk_size = 0x1000;
        f.set_len(disk_size).unwrap();
        let mem = Rc::new(
            GuestMemory::new(&[(GuestAddress(0u64), 4 * 1024 * 1024)])
                .expect("Creating guest memory failed."),
        );

        let req_hdr = virtio_blk_req_header {
            req_type: Le32::from(VIRTIO_BLK_T_IN),
            reserved: Le32::from(0),
            sector: Le64::from(7), // Disk is 8 sectors long, so this is the last valid sector.
        };
        mem.write_obj_at_addr(req_hdr, GuestAddress(0x1000))
            .expect("writing req failed");

        let avail_desc = create_descriptor_chain(
            &mem,
            GuestAddress(0x100),  // Place descriptor chain at 0x100.
            GuestAddress(0x1000), // Describe buffer at 0x1000.
            vec![
                // Request header
                (DescriptorType::Readable, size_of_val(&req_hdr) as u32),
                // I/O buffer (2 sectors of data - overlap the end of the disk).
                (DescriptorType::Writable, 512 * 2),
                // Request status
                (DescriptorType::Writable, 1),
            ],
            0,
        )
        .expect("create_descriptor_chain failed");

        let af = SingleFileDisk::try_from(f).expect("Failed to create SFD");
        let flush_timer_armed = Rc::new(AtomicBool::new(false));
        let disk_state = Rc::new(RefCell::new(DiskState {
            disk_image: Box::new(af),
            disk_size: AsyncMutex::new(Arc::new(AtomicU64::new(disk_size))),
            read_only: false,
            sparse: true,
        }));

        let fut = process_one_request(avail_desc, disk_state, Rc::clone(&flush_timer_armed), &mem);

        cros_async::run_one(Box::pin(fut))
            .expect("running executor failed")
            .expect("execute failed");

        let status_offset = GuestAddress((0x1000 + size_of_val(&req_hdr) + 512 * 2) as u64);
        let status = mem.read_obj_from_addr::<u8>(status_offset).unwrap();
        assert_eq!(status, VIRTIO_BLK_S_IOERR);
    }
}
