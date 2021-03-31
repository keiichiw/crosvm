// Copyright 2021 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

pub mod handler;

use std::fs::OpenOptions;
use std::sync::{Arc, Mutex};

use devices::virtio::{base_features, BlockAsync, Queue, VirtioDevice};
use devices::ProtectionType;
use disk::ToAsyncDisk;
use getopts::Options;
use handler::VhostUserDeviceReqHandler;
use vm_control::DiskControlResponseSocket;
use vmm_vhost::vhost_user::message::*;
use vmm_vhost::vhost_user::*;

use crate::handler::{VhostUserBackend, MAX_QUEUE_NUM, MAX_VRING_NUM};

struct BlockBackend {
    device: BlockAsync,
    acked_protocol_features: u64,
}

impl BlockBackend {
    fn new(
        base_features: u64,
        disk_image: Box<dyn ToAsyncDisk>,
        read_only: bool,
        sparse: bool,
        block_size: u32,
        control_socket: Option<DiskControlResponseSocket>,
    ) -> Self {
        let device = BlockAsync::new(
            base_features,
            disk_image,
            read_only,
            sparse,
            block_size,
            control_socket,
        )
        .expect("failed to create async block");

        Self {
            device,
            acked_protocol_features: 0,
        }
    }
}

impl VhostUserBackend for BlockBackend {
    fn protocol_features(&self) -> VhostUserProtocolFeatures {
        let mut features = VhostUserProtocolFeatures::all();
        features.remove(VhostUserProtocolFeatures::CONFIGURE_MEM_SLOTS);
        features.remove(VhostUserProtocolFeatures::INFLIGHT_SHMFD);
        features.remove(VhostUserProtocolFeatures::SLAVE_REQ);
        features
    }

    fn ack_protocol_features(&mut self, features: u64) {
        self.acked_protocol_features = features;
    }

    fn acked_protocol_features(&self) -> u64 {
        self.acked_protocol_features
    }

    fn reset(&mut self) {
        self.device.reset();
        self.acked_protocol_features = 0;
    }

    fn device(&self) -> &dyn VirtioDevice {
        &self.device
    }

    fn device_mut(&mut self) -> &mut dyn VirtioDevice {
        &mut self.device
    }
}

fn main() {
    if let Err(e) = base::syslog::init() {
        println!("failed to initialize syslog: {}", e);
        return;
    }

    let args: Vec<String> = std::env::args().collect();
    let mut opts = Options::new();
    opts.reqopt("", "disk", "Path to a disk image.", "PATH");
    opts.reqopt("", "socket", "Path to a socket", "PATH");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(e) => {
            println!("{}", e);
            println!("{}", opts.short_usage(&args[0]));
            return;
        }
    };
    let disk = matches.opt_str("disk").unwrap();
    let socket = matches.opt_str("socket").unwrap();
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(false)
        .open(disk)
        .unwrap();
    let block = BlockBackend::new(
        base_features(ProtectionType::Unprotected),
        Box::new(f),
        false, /*read-only*/
        false, /*sparse*/
        512,
        None,
    );
    let handler = Arc::new(std::sync::Mutex::new(VhostUserDeviceReqHandler::new(block)));
    let listener = Listener::new(socket, true).unwrap();
    let mut slave_listener = SlaveListener::new(listener, handler).unwrap();
    let mut listener = slave_listener.accept().unwrap().unwrap();
    loop {
        listener.handle_request().unwrap();
    }
}
