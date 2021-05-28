// Copyright (C) 2019 Alibaba Cloud Computing. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::cmp::{max, min};
use std::fs::OpenOptions;
use std::os::unix::io::RawFd;
use std::path::Path;
use std::rc::Rc;
use std::sync::{atomic::AtomicU64, atomic::Ordering, Arc};

use anyhow::{anyhow, bail, Context};
use futures::future::{AbortHandle, Abortable};
use getopts::Options;
use once_cell::sync::OnceCell;

use vmm_vhost::vhost_user::message::*;
use vmm_vhost::vhost_user::SlaveListener;
use vmm_vhost::vhost_user::*;

use base::{
    error, warn, Event, FromRawDescriptor, RawDescriptor, SafeDescriptor, SharedMemory,
    SharedMemoryUnix,
};
use cros_async::{
    select5, sync::Mutex as AsyncMutex, AsyncError, EventAsync, Executor, IoSourceExt,
    SelectResult, TimerAsync,
};
use devices::virtio;
use devices::virtio::block::{BlockId, DiskState, SECTOR_SIZE};
use devices::virtio::SignalableInterrupt;
use devices::virtio::{base_features, BlockAsync, Queue, VirtioDevice};
use devices::ProtectionType;
use disk::{create_async_disk_file, AsyncDisk, ToAsyncDisk};
use vhost_user_devices::{CallEvent, DeviceRequestHandler, VhostUserBackend};
use vm_memory::{GuestAddress, GuestMemory, MemoryRegion};

static BLOCK_EXECUTOR: OnceCell<Executor> = OnceCell::new();

pub struct BlockBackend {
    disk_state: Rc<AsyncMutex<DiskState>>,
    block_size: u32,
    avail_features: u64,
    acked_features: u64,
    acked_protocol_features: VhostUserProtocolFeatures,
    flush_timer: Rc<RefCell<TimerAsync>>,
    flush_timer_armed: Rc<RefCell<bool>>,
    workers: [Option<AbortHandle>; Self::MAX_QUEUE_NUM],
}

impl BlockBackend {
    pub fn new(
        disk_image: Box<dyn ToAsyncDisk>,
        base_features: u64,
        read_only: bool,
        sparse: bool,
        block_size: u32,
        id: Option<BlockId>,
    ) -> BlockBackend {
        if block_size % SECTOR_SIZE as u32 != 0 {
            panic!( // TODO error here
                "Block size {} is not a multiple of {}.",
                block_size, SECTOR_SIZE,
            );
        }
        let disk_size = disk_image.get_len().expect("Failed to get disk size");
        if disk_size % block_size as u64 != 0 {
            warn!(
                "Disk size {} is not a multiple of block size {}; \
                 the remainder will not be visible to the guest.",
                disk_size, block_size,
            );
        }

        let avail_features = build_avail_features(base_features, read_only, sparse, true);

        let seg_max = min(max(iov_max(), 1), u32::max_value() as usize) as u32;

        // Since we do not currently support indirect descriptors, the maximum
        // number of segments must be smaller than the queue size.
        // In addition, the request header and status each consume a descriptor.
        let seg_max = min(seg_max, u32::from(QUEUE_SIZE) - 2);

        let async_image = disk_image.to_async_disk(&ex).unwrap();

        let disk_state = Rc::new(AsyncMutex::new(DiskState::new(
            async_image,
            Arc::new(AtomicU64::new(disk_size)),
            read_only,
            sparse,
            id,
        )));

        let timer = Timer::new().expect("Failed to create a timer");
        let flush_timer = Rc::new(RefCell::new(
            TimerAsync::new(
                // Call try_clone() to share the same underlying FD with the `flush_disk` task.
                timer.0.try_clone().expect("Failed to clone flush_timer"),
                &ex,
            )
            .expect("Failed to create an async timer"),
        ));
        let flush_timer_armed = Rc::new(RefCell::new(false));

        BlockBackend {
            disk_state,
            block_size,
            avail_features,
            acked_features: 0,
            acked_protocol_features: VhostUserProtocolFeatures::empty(),
            flush_timer,
            flush_timer_armed,
            workers: [None; Self::MAX_QUEUE_NUM],
        }
    }
}

impl VhostUserBackend for BlockBackend {
    const MAX_QUEUE_NUM: usize = 16;
    const MAX_VRING_LEN: u16 = 256;

    type Error = anyhow::Error;

    fn features(&self) -> u64 {
        self.avail_features
    }

    fn ack_features(&mut self, value: u64) -> anyhow::Result<()> {
        let unrequested_features = value & !self.avail_features;
        if unrequested_features != 0 {
            bail!("invalid features are given: {:#x}", unrequested_features);
        }

        self.acked_features |= value;

        Ok(())
    }

    fn acked_features(&self) -> u64 {
        self.acked_features
    }

    fn protocol_features(&self) -> VhostUserProtocolFeatures {
        VhostUserProtocolFeatures::CONFIG | VhostUserProtocolFeatures::MQ
    }

    fn ack_protocol_features(&mut self, features: u64) -> anyhow::Result<()> {
        let features = VhostUserProtocolFeatures::from_bits(features)
            .ok_or_else(|| anyhow!("invalid protocol features are given: {:#x}", features))?;
        let supported = self.protocol_features();
        self.acked_protocol_features = features & supported;
        Ok(())
    }

    fn acked_protocol_features(&self) -> u64 {
        self.acked_protocol_features.bits()
    }

    fn read_config(&self, offset: u64, data: &mut [u8]) {
        println!("read_config, {} {}", offset, size);
        let config_space = {
            let disk_size = self.disk_state.disk_size.load(Ordering::Relaxed);
            build_config_space(
                disk_size,
                self.disk_state.seg_max,
                self.block_size,
                NUM_QUEUES,
            )
        };
        copy_config(data, 0, config_space.as_slice(), offset);
    }

    fn reset(&mut self) {
        panic!("Unsupported call to reset");
    }

    fn start_queue(
        &mut self,
        idx: usize,
        queue: virtio::Queue,
        mem: GuestMemory,
        call_evt: CallEvent,
        kick_evt: Event,
    ) -> anyhow::Result<()> {
        if let Some(handle) = self.workers.get_mut(idx).and_then(Option::take) {
            warn!("Starting new queue handler without stopping old handler");
            handle.abort();
        }

        // Safe because the executor is initialized in main() below.
        let ex = BLOCK_EXECUTOR.get().expect("Executor not initialized");

        let kick_evt =
            EventAsync::new(kick_evt.0, ex).context("failed to create EventAsync for kick_evt")?;
        let (handle, registration) = AbortHandle::new_pair();

        let disk_state = Rc::clone(&self.disk_state);
        let timer = Rc::clone(&self.flush_timer);
        let timer_armed = Rc::clone(&self.flush_timer_armed);
        ex.spawn_local(Abortable::new(
            handle_queue(
                mem,
                disk_state,
                Rc::new(RefCell::new(queue)),
                kick_evt,
                Rc::new(RefCell::new(call_event)),
                timer,
                timer_armed,
            ),
            registration,
        ))
        .detach();

        self.workers[idx] = Some(handle);
        Ok(())
    }

    fn stop_queue(&mut self, idx: usize) {
        if let Some(handle) = self.workers.get_mut(idx).and_then(Option::take) {
            handle.abort();
        }
    }
}

// There is one async task running `handle_queue` per virtio queue in use.
// Receives messages from the guest and queues a task to complete the operations with the async
// executor.
async fn handle_queue(
    mem: GuestMemory,
    disk_state: Rc<AsyncMutex<DiskState>>,
    queue: Rc<RefCell<Queue>>,
    evt: EventAsync,
    interrupt: Rc<RefCell<Box<dyn SignalableInterrupt + Send>>>,
    flush_timer: Rc<RefCell<TimerAsync>>,
    flush_timer_armed: Rc<RefCell<bool>>,
) {
    loop {
        if let Err(e) = evt.next_val().await {
            error!("Failed to read the next queue event: {}", e);
            continue;
        }
        while let Some(descriptor_chain) = queue.borrow_mut().pop(&mem) {
            ex.spawn_local(process_one_request_task(
                Rc::clone(&queue),
                descriptor_chain,
                Rc::clone(&disk_state),
                mem.clone(),
                Rc::clone(&interrupt),
                Rc::clone(&flush_timer),
                Rc::clone(&flush_timer_armed),
            ))
            .detach();
        }
    }
}

fn main() -> anyhow::Result<()> {
    let mut opts = Options::new();
    opts.reqopt("f", "file", "path to the disk file", "PATH");
    opts.optflag("h", "help", "print this help menu");
    opts.optflag("ro", "read-only", "expose disk as read only");
    opts.reqopt("", "socket", "path to a socket", "PATH");

    let mut args = std::env::args();
    let program_name = args.next().expect("args is empty");
    let matches = match opts.parse(args) {
        Ok(m) => m,
        Err(e) => {
            println!("{}", e);
            println!("{}", opts.short_usage(&program_name));
            return Ok(());
        }
    };

    if matches.opt_present("h") {
        println!("{}", opts.usage(&program_name));
        return Ok(());
    }

    base::syslog::init().context("failed to initialize syslog")?;

    let ex = Executor::new().context("failed to create executor")?;

    // We can unwrap after `opt_str()` safely because they are required options.
    let socket = matches.opt_str("socket").unwrap();
    let filename = matches.opt_str("file").unwrap();
    let read_only = matches.opt_present("file");
    let sparse = true;
    let f = OpenOptions::new()
        .read(true)
        .write(!read_only)
        .create(false)
        .open(filename)
        .unwrap();
    let async_file = Box::new(create_async_disk_file(f).unwrap());

    let _ = BLOCK_EXECUTOR.set(ex.clone());

    let base_features = base_features(ProtectionType::Unprotected);
    let block = BlockBackend::new(
        disk_image,
        base_features,
        read_only,
        sparse,
        block_size,
        None, // id: Option<BlockId>,
    );
    let handler = DeviceRequestHandler::new(block);

    if let Err(e) = ex.run_until(handler.run(socket, &ex)) {
        error!("error occurred: {}", e);
    }

    Ok(())
}
