// Copyright 2021 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

pub mod handler;

use std::net::Ipv4Addr;
use std::sync::Arc;

use devices::virtio::{base_features, Net, VirtioDevice};
use devices::ProtectionType;
use getopts::Options;
use handler::VhostUserDeviceReqHandler;
use net_util::{MacAddress, Tap};
use vmm_vhost::vhost_user::message::*;
use vmm_vhost::vhost_user::*;

use crate::handler::VhostUserBackend;

struct NetBackend {
    device: Net<Tap>,
    acked_protocol_features: u64,
}

impl NetBackend {
    pub fn new(
        base_features: u64,
        host_ip: Ipv4Addr,
        netmask: Ipv4Addr,
        mac_address: MacAddress,
        vq_pairs: u16,
    ) -> Self {
        let device = Net::<Tap>::new(base_features, host_ip, netmask, mac_address, vq_pairs)
            .expect("failed to create net device");
        Self {
            device,
            acked_protocol_features: 0,
        }
    }
}

impl VhostUserBackend for NetBackend {
    fn expected_queue_num(&self) -> usize {
        2
    }

    fn protocol_features(&self) -> VhostUserProtocolFeatures {
        let mut features = VhostUserProtocolFeatures::all();
        features.remove(VhostUserProtocolFeatures::CONFIGURE_MEM_SLOTS);
        features.remove(VhostUserProtocolFeatures::INFLIGHT_SHMFD);
        features.remove(VhostUserProtocolFeatures::SLAVE_REQ);
        features
    }

    fn ack_protocol_features(&mut self, features: u64) {
        // Note: slave that reported VHOST_USER_F_PROTOCOL_FEATURES must
        // support this message even before VHOST_USER_SET_FEATURES was
        // called.
        // What happens if the master calls set_features() with
        // VHOST_USER_F_PROTOCOL_FEATURES cleared after calling this
        // interface?
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
    opts.reqopt(
        "",
        "host_ip",
        "IP address to assign to host tap interface.",
        "IP",
    );
    opts.reqopt("", "netmask", "Netmask for VM subnet.", "NETMASK");
    opts.reqopt("", "mac", "MAC address for VM.", "MAC");
    opts.reqopt("", "socket", "Path to a socket", "PATH");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(e) => {
            println!("{}", e);
            println!("{}", opts.short_usage(&args[0]));
            return;
        }
    };

    let features = base_features(ProtectionType::Unprotected);
    let host_ip: Ipv4Addr = matches.opt_str("host_ip").unwrap().parse().unwrap();
    let netmask: Ipv4Addr = matches.opt_str("netmask").unwrap().parse().unwrap();
    let mac_address: MacAddress = matches.opt_str("mac").unwrap().parse().unwrap();
    let socket = matches.opt_str("socket").unwrap();
    let vq_pairs = 1;

    let net = NetBackend::new(features, host_ip, netmask, mac_address, vq_pairs);
    let handler = Arc::new(std::sync::Mutex::new(VhostUserDeviceReqHandler::new(net)));
    let listener = Listener::new(socket, true).unwrap();
    let mut slave_listener = SlaveListener::new(listener, handler).unwrap();
    println!("waiting");
    let mut listener = slave_listener.accept().unwrap().unwrap();
    println!("accepted");
    loop {
        listener.handle_request().unwrap();
    }
}
