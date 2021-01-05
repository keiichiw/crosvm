// Copyright (C) 2019 Alibaba Cloud Computing. All rights reserved.
// SPDX-License-Identifier: Apache-2.0
extern crate vhost;

use std::cmp::{max, min};
use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex};

use vmm_vhost::vhost_user::message::*;
use vmm_vhost::vhost_user::*;

use base::iov_max;
use base::{FromRawDescriptor, MemoryMapping, MemoryMappingBuilder, RawDescriptor, SafeDescriptor};
use data_model::{DataInit, Le16, Le32, Le64};

use devices::virtio::block::{build_config_space, virtio_blk_config};

pub const MAX_QUEUE_NUM: usize = 2;
pub const MAX_VRING_NUM: usize = 256;
pub const VIRTIO_FEATURES: u64 = 0x4000_0003;

#[derive(Default)]
struct QueueInfo {
    descriptor_table: u64,
    avail_ring: u64,
    used_ring: u64,
}

#[derive(Default)]
pub struct BlockSlaveReqHandler {
    pub owned: bool,
    pub features_acked: bool,
    pub acked_features: u64,
    pub acked_protocol_features: u64,
    pub queue_num: usize,
    pub vring_num: [u32; MAX_QUEUE_NUM],
    pub vring_base: [u32; MAX_QUEUE_NUM],
    queue_info: [QueueInfo; MAX_QUEUE_NUM],
    pub call_fd: [Option<RawFd>; MAX_QUEUE_NUM],
    pub kick_fd: [Option<RawFd>; MAX_QUEUE_NUM],
    pub err_fd: [Option<RawFd>; MAX_QUEUE_NUM],
    pub vring_started: [bool; MAX_QUEUE_NUM],
    pub vring_enabled: [bool; MAX_QUEUE_NUM],
    vu_req: Option<SlaveFsCacheReq>,
    mem_tables: Option<Vec<MemoryMapping>>,
}

impl BlockSlaveReqHandler {
    pub fn new() -> Self {
        BlockSlaveReqHandler {
            queue_num: MAX_QUEUE_NUM,
            ..Default::default()
        }
    }

    fn vmm_va_to_gpa(&self, vmm_va: u64) -> VhostUserHandlerResult<u64> {
        if let Some(memory) = &self.memory {
            for mapping in memory.mappings.iter() {
                if vmm_va >= mapping.vmm_addr && vmm_va < mapping.vmm_addr + mapping.size {
                    return Ok(vmm_va - mapping.vmm_addr + mapping.gpa_base);
                }
            }
        }

        Err(VhostUserHandlerError::MissingMemoryMapping)
    }
}

impl VhostUserSlaveReqHandler for BlockSlaveReqHandler {
    fn set_owner(&mut self) -> Result<()> {
        println!("set_owner");
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
        self.acked_features = 0;
        self.acked_protocol_features = 0;
        Ok(())
    }

    fn get_features(&mut self) -> Result<u64> {
        println!("get_features {:x}", VIRTIO_FEATURES);
        // dg-- qemu doesn't seem to ack, so assume features are enabled.
        //        self.acked_features = VIRTIO_FEATURES;
        Ok(VIRTIO_FEATURES)
    }

    fn set_features(&mut self, features: u64) -> Result<()> {
        println!("set_features");
        if !self.owned {
            println!("set_features unowned");
            return Err(Error::InvalidOperation);
        } else if (features & !VIRTIO_FEATURES) != 0 {
            println!("set_features no features");
            return Err(Error::InvalidParam);
        }

        self.acked_features = features;
        self.features_acked = true;

        // If VHOST_USER_F_PROTOCOL_FEATURES has not been negotiated,
        // the ring is initialized in an enabled state.
        // If VHOST_USER_F_PROTOCOL_FEATURES has been negotiated,
        // the ring is initialized in a disabled state. Client must not
        // pass data to/from the backend until ring is enabled by
        // VHOST_USER_SET_VRING_ENABLE with parameter 1, or after it has
        // been disabled by VHOST_USER_SET_VRING_ENABLE with parameter 0.
        let vring_enabled =
            self.acked_features & VhostUserVirtioFeatures::PROTOCOL_FEATURES.bits() == 0;
        for enabled in &mut self.vring_enabled {
            *enabled = vring_enabled;
        }

        Ok(())
    }

    fn get_protocol_features(&mut self) -> Result<VhostUserProtocolFeatures> {
        println!("get_protocol_features");
        let mut features = VhostUserProtocolFeatures::all();
        features.remove(VhostUserProtocolFeatures::CONFIGURE_MEM_SLOTS);
        features.remove(VhostUserProtocolFeatures::INFLIGHT_SHMFD);
        features.remove(VhostUserProtocolFeatures::SLAVE_REQ);
        Ok(features)
    }

    fn set_protocol_features(&mut self, features: u64) -> Result<()> {
        println!("set_protocol_features");
        // Note: slave that reported VHOST_USER_F_PROTOCOL_FEATURES must
        // support this message even before VHOST_USER_SET_FEATURES was
        // called.
        // What happens if the master calls set_features() with
        // VHOST_USER_F_PROTOCOL_FEATURES cleared after calling this
        // interface?
        self.acked_protocol_features = features;
        Ok(())
    }

    fn set_mem_table(&mut self, contexts: &[VhostUserMemoryRegion], fds: &[RawFd]) -> Result<()> {
        println!("set_mem_table:");
        for c in contexts.iter() {
            println!("    {:x} {:x}", c.memory_size, c.mmap_offset);
        }
        self.mem_tables = Some(
            contexts
                .iter()
                .zip(fds.iter())
                .map(|(ctx, &fd)| {
                    let sd = unsafe { SafeDescriptor::from_raw_descriptor(fd as RawDescriptor) };
                    MemoryMappingBuilder::new(ctx.memory_size as usize)
                        .from_descriptor(&sd)
                        .offset(ctx.mmap_offset)
                        .build()
                        .unwrap()
                })
                .collect(),
        );
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
        let queue = &mut self.queue_info[index as usize];
        queue.descriptor_table = self.vmm_va_to_gpa(descriptor);
        queue.avail_ring = self.vmm_va_to_gpa(available);
        queue.used_ring = self.vmm_va_to_gpa(used);
        Ok(())
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
        self.vring_started[index as usize] = false;
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
        if self.kick_fd[index as usize].is_some() {
            // Close file descriptor set by previous operations.
            let _ = unsafe { libc::close(self.kick_fd[index as usize].unwrap()) };
        }
        self.kick_fd[index as usize] = fd;

        // Quotation from vhost-user spec:
        // Client must start ring upon receiving a kick (that is, detecting
        // that file descriptor is readable) on the descriptor specified by
        // VHOST_USER_SET_VRING_KICK, and stop ring upon receiving
        // VHOST_USER_GET_VRING_BASE.
        //
        // So we should add fd to event monitor(select, poll, epoll) here.
        self.vring_started[index as usize] = true;
        Ok(())
    }

    fn set_vring_call(&mut self, index: u8, fd: Option<RawFd>) -> Result<()> {
        println!("set_vring_call");
        if index as usize >= self.queue_num || index as usize > self.queue_num {
            return Err(Error::InvalidParam);
        }
        if self.call_fd[index as usize].is_some() {
            // Close file descriptor set by previous operations.
            let _ = unsafe { libc::close(self.call_fd[index as usize].unwrap()) };
        }
        self.call_fd[index as usize] = fd;
        Ok(())
    }

    fn set_vring_err(&mut self, index: u8, fd: Option<RawFd>) -> Result<()> {
        println!("set_vring_err");
        if index as usize >= self.queue_num || index as usize > self.queue_num {
            return Err(Error::InvalidParam);
        }
        if self.err_fd[index as usize].is_some() {
            // Close file descriptor set by previous operations.
            let _ = unsafe { libc::close(self.err_fd[index as usize].unwrap()) };
        }
        self.err_fd[index as usize] = fd;
        Ok(())
    }

    fn set_vring_enable(&mut self, index: u32, enable: bool) -> Result<()> {
        println!("vring_enable");
        // This request should be handled only when VHOST_USER_F_PROTOCOL_FEATURES
        // has been negotiated.
        if self.acked_features & VhostUserVirtioFeatures::PROTOCOL_FEATURES.bits() == 0 {
            return Err(Error::InvalidOperation);
        } else if index as usize >= self.queue_num || index as usize > self.queue_num {
            return Err(Error::InvalidParam);
        }

        // Slave must not pass data to/from the backend until ring is
        // enabled by VHOST_USER_SET_VRING_ENABLE with parameter 1,
        // or after it has been disabled by VHOST_USER_SET_VRING_ENABLE
        // with parameter 0.
        self.vring_enabled[index as usize] = enable;
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
        let seg_max = min(max(iov_max(), 1), u32::max_value() as usize) as u32;
        let config = build_config_space(1024 * 1024 * 1024, seg_max, 1024);
        println!("get_config OK");
        Ok(config.as_slice()[..size as usize].iter().cloned().collect())
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
}

fn main() -> Result<()> {
    let backend = Arc::new(Mutex::new(BlockSlaveReqHandler::new()));
    let listener = Listener::new("/tmp/vhost_user_blk.socket", true).unwrap();
    let mut slave_listener = SlaveListener::new(listener, backend).unwrap();
    let mut listener = slave_listener.accept().unwrap().unwrap();
    loop {
        listener.handle_request().unwrap();
    }
    Ok(())
}
