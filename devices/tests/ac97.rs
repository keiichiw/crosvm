// Copyright 2018 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

extern crate devices;
extern crate sys_util;

use devices::Ac97;

#[test]
fn bm_bdbar() {
    let mut ac97 = Ac97::new();

    let bdbars = [0x00u64, 0x10, 0x20];

    // Tests that the base address is writable and that the bottom three bits are read only.
    for bdbar in &bdbars {
        assert_eq!(ac97.bm_readl(*bdbar), 0x00);
        ac97.bm_writel(*bdbar, 0x5555_555f);
        assert_eq!(ac97.bm_readl(*bdbar), 0x5555_5558);
    }
}

#[test]
fn bm_status_reg() {
    let mut ac97 = Ac97::new();

    let sr_addrs = [0x06u64, 0x16, 0x26];

    for sr in &sr_addrs {
        assert_eq!(ac97.bm_readw(*sr), 0x0001);
        ac97.bm_writew(*sr, 0xffff);
        assert_eq!(ac97.bm_readw(*sr), 0x0001);
    }
}

#[test]
fn bm_global_control() {
    let mut ac97 = Ac97::new();

    let glob_cnt = 0x2cu64;

    assert_eq!(ac97.bm_readl(glob_cnt), 0x0000_0000);

    // Check interrupt enable bits are writable.
    ac97.bm_writel(glob_cnt, 0x0000_0076);
    assert_eq!(ac97.bm_readl(glob_cnt), 0x0000_0076);

    // Check that a soft reset works, but setting bdbar and checking it is zeroed.
    ac97.bm_writel(0x00, 0x5555_555f);
    ac97.bm_writel(glob_cnt, 0x000_0074);
    assert_eq!(ac97.bm_readl(glob_cnt), 0x0000_0074);
    assert_eq!(ac97.bm_readl(0x00), 0x0000_0000);
}