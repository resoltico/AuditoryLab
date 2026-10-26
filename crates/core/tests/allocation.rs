// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Rendering allocates nothing, measured on the current thread from the
//! first frame through the silent tail.

#![cfg(test)]

use auditorylab_core::{Recipe, SampleRate, catalog};
use std::hint::black_box;

#[test]
fn rendering_any_catalog_entry_allocates_nothing() {
    for entry in catalog::ENTRIES {
        for hz in [44_100, 48_000] {
            let mut sound = Recipe::parse(entry.text).unwrap().sound().clone();
            sound.duration_seconds = 0.5;
            let signal = sound.signal(SampleRate::new(hz).unwrap()).unwrap();
            let mut buffers = [
                vec![],
                vec![[1.0; 2]; 1],
                vec![[1.0; 2]; 17],
                vec![[1.0; 2]; 4_099],
                vec![[1.0; 2]; 30_000],
            ];
            let counts = allocation_counter::measure(|| {
                for _ in 0..2 {
                    for buffer in &mut buffers {
                        black_box(signal.render(0, buffer));
                    }
                }
            });
            assert_eq!(
                (counts.count_total, counts.bytes_total),
                (0, 0),
                "{}",
                entry.id
            );
        }
    }
}

#[test]
fn the_measurement_sees_an_allocation() {
    let counts = allocation_counter::measure(|| {
        black_box(vec![[0.0_f64; 2]; 16]);
    });
    assert!(counts.count_total > 0);
}
