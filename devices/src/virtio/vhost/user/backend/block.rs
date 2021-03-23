// Copyright 2021 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

pub mod handler;

use std::fs::OpenOptions;
use std::sync::Arc;

use devices::virtio::{base_features, BlockAsync};
use devices::ProtectionType;
use handler::DevReqHandler;
use vmm_vhost::vhost_user::*;

fn main() {
    if let Err(e) = base::syslog::init() {
        println!("failed to initialize syslog: {}", e);
        return;
    }

    let args: Vec<String> = std::env::args().collect();
    let filename = &args[1];
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(false)
        .open(filename)
        .unwrap();
    let block = BlockAsync::new(
        base_features(ProtectionType::Unprotected),
        Box::new(f),
        false, /*read-only*/
        false, /*sparse*/
        512,
        None,
    )
    .unwrap();

    let backend = Arc::new(std::sync::Mutex::new(DevReqHandler::new(block)));
    let listener = Listener::new("/tmp/vhost.socket", true).unwrap();
    let mut slave_listener = SlaveListener::new(listener, backend).unwrap();
    let mut listener = slave_listener.accept().unwrap().unwrap();
    loop {
        listener.handle_request().unwrap();
    }
}
