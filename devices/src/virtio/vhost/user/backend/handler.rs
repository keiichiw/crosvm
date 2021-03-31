use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex};

use vmm_vhost::vhost_user::message::*;
use vmm_vhost::vhost_user::*;

use base::{
    error, Event, FromRawDescriptor, RawDescriptor, SafeDescriptor, SharedMemory, SharedMemoryUnix,
};
use vm_memory::{GuestAddress, GuestMemory, MemoryRegion};

use devices::virtio::{Queue, SignalableInterrupt, VirtioDevice};

pub const MAX_QUEUE_NUM: usize = 2;
pub const MAX_VRING_NUM: usize = 256;
pub const VIRTIO_TRANSPORT_FEATURES: u64 = 0x1_4000_0000;

struct CallEvent(Event);

impl SignalableInterrupt for CallEvent {
    /// Writes to the irqfd to VMM to deliver virtual interrupt to the guest.
    fn signal(&self, _vector: u16, interrupt_status_mask: u32) {
        self.0.write(interrupt_status_mask as u64).unwrap();
    }

    /// Notify the driver that buffers have been placed in the used queue.
    fn signal_used_queue(&self, vector: u16) {
        self.signal(vector, 0 /*INTERRUPT_STATUS_USED_RING*/)
    }

    /// Notify the driver that the device configuration has changed.
    fn signal_config_changed(&self) {} // TODO(dgreid)

    /// Get the event to signal resampling is needed if it exists.
    fn get_resample_evt(&self) -> Option<&Event> {
        None
    }

    /// Reads the status and writes to the interrupt event. Doesn't read the resample event, it
    /// assumes the resample has been requested.
    fn do_interrupt_resample(&self) {}
}

/// Keeps a mpaaing from the vmm's virtual addresses to guest addresses.
/// used to translate messages from the vmm to guest offsets.
#[derive(Default)]
struct MappingInfo {
    vmm_addr: u64,
    guest_phys: u64,
    size: u64,
}

fn vmm_va_to_gpa(maps: &Vec<MappingInfo>, vmm_va: u64) -> Result<u64> {
    for map in maps {
        if vmm_va >= map.vmm_addr && vmm_va < map.vmm_addr + map.size {
            return Ok(vmm_va - map.vmm_addr + map.guest_phys);
        }
    }
    Err(Error::InvalidMessage)
}

struct MemInfo {
    guest_mem: GuestMemory,
    vmm_maps: Vec<MappingInfo>,
}

pub trait VhostUserBackend {
    fn protocol_features(&self) -> VhostUserProtocolFeatures;
    fn ack_protocol_features(&mut self, _value: u64);
    fn acked_protocol_features(&self) -> u64;
    fn reset(&mut self);

    fn device(&self) -> &dyn VirtioDevice;
    fn device_mut(&mut self) -> &mut dyn VirtioDevice;

    fn queues_per_worker(&self) -> Vec<Vec<usize>> {
        self.device().queues_per_worker()
    }

    fn features(&self) -> u64 {
        self.device().features()
    }

    fn ack_features(&mut self, value: u64) {
        self.device_mut().ack_features(value)
    }

    fn acked_features(&self) -> u64 {
        self.device().acked_features()
    }

    fn read_config(&self, offset: u64, data: &mut [u8]) {
        self.device().read_config(offset, data)
    }

    fn activate_vhost(
        &mut self,
        mem: GuestMemory,
        interrupts: Vec<Box<dyn SignalableInterrupt + Send>>,
        queues: Vec<Arc<Mutex<Queue>>>,
        queue_evts: Vec<Event>,
    ) {
        self.device_mut()
            .activate_vhost(mem, interrupts, queues, queue_evts)
    }
}

pub struct Vring {
    queue: Arc<Mutex<Queue>>,
    call_fd: Option<Event>,
    kick_fd: Option<Event>,
    enabled: bool,

    /// Whether a backend is activated with the queue.
    activated: bool,
}

impl Vring {
    fn new(max_size: u16) -> Self {
        Self {
            queue: Arc::new(Mutex::new(Queue::new(max_size))),
            call_fd: None,
            kick_fd: None,
            enabled: false,
            activated: false,
        }
    }

    fn reset(&mut self) {
        let mut queue = self.queue.lock().expect("failed to get queue");
        queue.reset();
        self.call_fd = None;
        self.kick_fd = None;
        self.enabled = false;
        self.activated = false;
    }
}

pub struct VhostUserDeviceReqHandler<B: VhostUserBackend> {
    pub owned: bool,
    pub features_acked: bool,
    pub queue_num: usize,
    pub vring_num: [u32; MAX_QUEUE_NUM],
    pub vring_base: [u32; MAX_QUEUE_NUM],
    pub err_fd: [Option<Event>; MAX_QUEUE_NUM],
    vu_req: Option<SlaveFsCacheReq>,
    mem: Option<MemInfo>,
    vrings: Vec<Vring>,
    backend: B,
}

impl<B: VhostUserBackend> VhostUserDeviceReqHandler<B> {
    pub fn new(backend: B) -> Self {
        let mut vrings = Vec::with_capacity(MAX_QUEUE_NUM as usize);
        for _ in 0..MAX_QUEUE_NUM {
            vrings.push(Vring::new(MAX_VRING_NUM as u16));
        }

        VhostUserDeviceReqHandler {
            owned: false,
            features_acked: false,
            queue_num: MAX_QUEUE_NUM,
            vring_num: [0; MAX_QUEUE_NUM],
            vring_base: [0; MAX_QUEUE_NUM],
            err_fd: Default::default(),
            vu_req: None,
            mem: None,
            vrings,
            backend,
        }
    }

    fn potentially_start_dev(&mut self, index: usize) {
        if self.vrings[index].activated {
            return;
        }

        let qs_per_worker = self.backend.queues_per_worker();

        let queues = if let Some(qs) = qs_per_worker
            .iter()
            .find(|qs| qs.iter().find(|q| **q == index).is_some())
        {
            qs
        } else {
            return;
        };

        for q_index in queues {
            if self.vrings[*q_index].call_fd.is_none()
                || self.vrings[*q_index].kick_fd.is_none()
                || !self.vrings[*q_index].enabled
            {
                return;
            }
        }

        self.activate(&queues);
    }

    fn activate(&mut self, queue_indexes: &[usize]) {
        let call_events = queue_indexes
            .iter()
            .map(|index| self.vrings[*index].call_fd.take().expect("call_fd"))
            .map(|call_fd| Box::<CallEvent>::new(CallEvent(call_fd)))
            .map(|call_event| Box::<dyn SignalableInterrupt + Send>::from(call_event))
            .collect::<Vec<_>>();
        let kick_fds = queue_indexes
            .iter()
            .map(|index| self.vrings[*index].kick_fd.take().expect("kick_fd"))
            .collect();
        let queues = queue_indexes
            .iter()
            .map(|index| self.vrings[*index].queue.clone())
            .collect();

        self.backend.activate_vhost(
            self.mem.as_ref().unwrap().guest_mem.clone(),
            call_events,
            queues,
            kick_fds,
        );

        for index in queue_indexes {
            self.vrings[*index].activated = true;
        }
    }
}

impl<B: VhostUserBackend> VhostUserSlaveReqHandlerMut for VhostUserDeviceReqHandler<B> {
    fn set_owner(&mut self) -> Result<()> {
        error!("set_owner");
        if self.owned {
            return Err(Error::InvalidOperation);
        }
        self.owned = true;
        Ok(())
    }

    fn reset_owner(&mut self) -> Result<()> {
        println!("reset_owner");
        self.owned = false;
        self.features_acked = false;
        self.backend.reset();
        Ok(())
    }

    fn get_features(&mut self) -> Result<u64> {
        let features = self.backend.features() | VIRTIO_TRANSPORT_FEATURES;
        println!("get_features {:x}", features);
        Ok(features)
    }

    fn set_features(&mut self, features: u64) -> Result<()> {
        println!("set_features");
        if !self.owned {
            println!("set_features unowned");
            return Err(Error::InvalidOperation);
        } else if (features & !(self.backend.features() | VIRTIO_TRANSPORT_FEATURES)) != 0 {
            println!("set_features no features");
            return Err(Error::InvalidParam);
        }

        self.backend.ack_features(features);
        self.features_acked = true;

        // If VHOST_USER_F_PROTOCOL_FEATURES has not been negotiated,
        // the ring is initialized in an enabled state.
        // If VHOST_USER_F_PROTOCOL_FEATURES has been negotiated,
        // the ring is initialized in a disabled state. Client must not
        // pass data to/from the backend until ring is enabled by
        // VHOST_USER_SET_VRING_ENABLE with parameter 1, or after it has
        // been disabled by VHOST_USER_SET_VRING_ENABLE with parameter 0.
        let vring_enabled =
            self.backend.acked_features() & VhostUserVirtioFeatures::PROTOCOL_FEATURES.bits() == 0;
        for v in &mut self.vrings {
            v.enabled = vring_enabled;
        }

        Ok(())
    }

    fn get_protocol_features(&mut self) -> Result<VhostUserProtocolFeatures> {
        println!("get_protocol_features");
        Ok(self.backend.protocol_features())
    }

    fn set_protocol_features(&mut self, features: u64) -> Result<()> {
        println!("set_protocol_features");
        self.backend.ack_protocol_features(features);
        Ok(())
    }

    fn set_mem_table(&mut self, contexts: &[VhostUserMemoryRegion], fds: &[RawFd]) -> Result<()> {
        println!("set_mem_table:");
        for c in contexts.iter() {
            let size = c.memory_size;
            let offset = c.mmap_offset;
            println!("    {:x} {:x}", size, offset);
        }
        if fds.len() != contexts.len() {
            println!("set_mem_table: mismatched fd/context count.");
            return Err(Error::InvalidParam);
        }

        let regions = contexts
            .iter()
            .zip(fds.iter())
            .map(|(region, &fd)| {
                let sd = unsafe { SafeDescriptor::from_raw_descriptor(fd as RawDescriptor) };
                MemoryRegion::new(
                    region.memory_size,
                    GuestAddress(region.guest_phys_addr),
                    region.mmap_offset,
                    Arc::new(SharedMemory::from_safe_descriptor(sd).unwrap()),
                )
                .unwrap()
            })
            .collect();
        let guest_mem = GuestMemory::from_regions(regions).unwrap();

        let vmm_maps = contexts
            .iter()
            .map(|region| MappingInfo {
                vmm_addr: region.user_addr,
                guest_phys: region.guest_phys_addr,
                size: region.memory_size,
            })
            .collect();
        self.mem = Some(MemInfo {
            guest_mem,
            vmm_maps,
        });
        Ok(())
    }

    fn get_queue_num(&mut self) -> Result<u64> {
        println!("get_queue_num");
        Ok(MAX_QUEUE_NUM as u64)
    }

    fn set_vring_num(&mut self, index: u32, num: u32) -> Result<()> {
        println!("set_vring_num");
        if index as usize >= self.queue_num || num == 0 || num as usize > MAX_VRING_NUM {
            return Err(Error::InvalidParam);
        }
        self.vring_num[index as usize] = num;

        Ok(())
    }

    fn set_vring_addr(
        &mut self,
        index: u32,
        flags: VhostUserVringAddrFlags,
        descriptor: u64,
        used: u64,
        available: u64,
        log: u64,
    ) -> Result<()> {
        println!(
            "set_vring_addr index:{} flags:{:x} desc:{:x} used:{:x} avail:{:x} log:{:x}",
            index, flags, descriptor, used, available, log
        );
        if index as usize >= self.queue_num {
            return Err(Error::InvalidParam);
        }
        if let Some(mem) = &self.mem {
            let mut queue = self.vrings[index as usize]
                .queue
                .lock()
                .expect("failed to get queue");
            queue.desc_table = GuestAddress(vmm_va_to_gpa(&mem.vmm_maps, descriptor).unwrap());
            queue.avail_ring = GuestAddress(vmm_va_to_gpa(&mem.vmm_maps, available).unwrap());
            queue.used_ring = GuestAddress(vmm_va_to_gpa(&mem.vmm_maps, used).unwrap());
            return Ok(());
        }
        return Err(Error::InvalidParam);
    }

    fn set_vring_base(&mut self, index: u32, base: u32) -> Result<()> {
        println!("set_vring_base");
        if index as usize >= self.queue_num || base as usize >= MAX_VRING_NUM {
            return Err(Error::InvalidParam);
        }
        self.vring_base[index as usize] = base;
        Ok(())
    }

    fn get_vring_base(&mut self, index: u32) -> Result<VhostUserVringState> {
        println!("get_vring_base");
        if index as usize >= self.queue_num {
            return Err(Error::InvalidParam);
        }
        // Quotation from vhost-user spec:
        // Client must start ring upon receiving a kick (that is, detecting
        // that file descriptor is readable) on the descriptor specified by
        // VHOST_USER_SET_VRING_KICK, and stop ring upon receiving
        // VHOST_USER_GET_VRING_BASE.
        for v in self.vrings.iter_mut() {
            v.reset();
        }

        Ok(VhostUserVringState::new(
            index,
            self.vring_base[index as usize],
        ))
    }

    fn set_vring_kick(&mut self, index: u8, fd: Option<RawFd>) -> Result<()> {
        println!("set_vring_kick");
        if index as usize >= self.queue_num || index as usize > self.queue_num {
            return Err(Error::InvalidParam);
        }
        unsafe {
            // Safe because the FD is now owned.
            self.vrings[index as usize].kick_fd = fd.map(|fd| Event::from_raw_descriptor(fd));
        }

        // Quotation from vhost-user spec:
        // Client must start ring upon receiving a kick (that is, detecting
        // that file descriptor is readable) on the descriptor specified by
        // VHOST_USER_SET_VRING_KICK, and stop ring upon receiving
        // VHOST_USER_GET_VRING_BASE.
        //
        // So we should add fd to event monitor(select, poll, epoll) here.
        {
            let mut queue = self.vrings[index as usize]
                .queue
                .lock()
                .expect("failed to get queue");
            queue.ready = true;
        }
        // TODO:dgreid - call activate here? or wait until both kick,call,and err have been set?
        self.potentially_start_dev(index as usize);
        Ok(())
    }

    fn set_vring_call(&mut self, index: u8, fd: Option<RawFd>) -> Result<()> {
        println!("set_vring_call {}", index);
        if index as usize >= self.queue_num || index as usize > self.queue_num {
            return Err(Error::InvalidParam);
        }
        unsafe {
            // Safe because the FD is now owned.
            self.vrings[index as usize].call_fd = fd.map(|fd| Event::from_raw_descriptor(fd));
        }
        self.potentially_start_dev(index as usize);
        Ok(())
    }

    fn set_vring_err(&mut self, index: u8, fd: Option<RawFd>) -> Result<()> {
        println!("set_vring_err");
        if index as usize >= self.queue_num || index as usize > self.queue_num {
            return Err(Error::InvalidParam);
        }
        unsafe {
            // Safe because the FD is now owned.
            self.err_fd[index as usize] = fd.map(|fd| Event::from_raw_descriptor(fd));
        }
        Ok(())
    }

    fn set_vring_enable(&mut self, index: u32, enable: bool) -> Result<()> {
        println!("vring_enable: index={}, enable={}", index, enable);
        // This request should be handled only when VHOST_USER_F_PROTOCOL_FEATURES
        // has been negotiated.
        if self.backend.acked_features() & VhostUserVirtioFeatures::PROTOCOL_FEATURES.bits() == 0 {
            return Err(Error::InvalidOperation);
        } else if index as usize >= self.queue_num || index as usize > self.queue_num {
            return Err(Error::InvalidParam);
        }

        // Slave must not pass data to/from the backend until ring is
        // enabled by VHOST_USER_SET_VRING_ENABLE with parameter 1,
        // or after it has been disabled by VHOST_USER_SET_VRING_ENABLE
        // with parameter 0.
        self.vrings[index as usize].enabled = enable;
        self.potentially_start_dev(index as usize);
        Ok(())
    }

    fn get_config(
        &mut self,
        offset: u32,
        size: u32,
        _flags: VhostUserConfigFlags,
    ) -> Result<Vec<u8>> {
        println!("get_config, {} {}", offset, size);
        if offset >= VHOST_USER_CONFIG_SIZE || size + offset > VHOST_USER_CONFIG_SIZE {
            return Err(Error::InvalidParam);
        }
        let mut data = vec![0; size as usize];
        self.backend.read_config(u64::from(offset), &mut data);
        Ok(data)
    }

    fn set_config(&mut self, offset: u32, buf: &[u8], _flags: VhostUserConfigFlags) -> Result<()> {
        let size = buf.len() as u32;
        println!("set_config {}", size);
        if offset < VHOST_USER_CONFIG_OFFSET
            || offset >= VHOST_USER_CONFIG_SIZE
            || size > VHOST_USER_CONFIG_SIZE - VHOST_USER_CONFIG_OFFSET
            || size + offset > VHOST_USER_CONFIG_SIZE
        {
            return Err(Error::InvalidParam);
        }
        Ok(())
    }

    fn set_slave_req_fd(&mut self, vu_req: SlaveFsCacheReq) {
        self.vu_req = Some(vu_req);
    }

    fn get_max_mem_slots(&mut self) -> Result<u64> {
        //TODO
        Ok(0)
    }

    fn add_mem_region(&mut self, _region: &VhostUserSingleMemoryRegion, _fd: RawFd) -> Result<()> {
        //TODO
        Ok(())
    }

    fn remove_mem_region(&mut self, _region: &VhostUserSingleMemoryRegion) -> Result<()> {
        //TODO
        Ok(())
    }
}
