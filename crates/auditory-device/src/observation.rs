// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Lossy preallocated observation, separate from reliable playback control.
use auditory_core::{RenderedSpan, SampleRate};
use std::{
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// One observation publication per existing fixed audio scratch chunk.
pub const OBSERVATION_FRAMES: usize = 1_024;
/// Heap-free payload containing only the active generated prefix.
pub struct ObservationPacket {
    start: u64,
    frames: usize,
    samples: [[f64; 2]; OBSERVATION_FRAMES],
}
impl ObservationPacket {
    /// First included source frame.
    #[must_use]
    pub const fn start(&self) -> u64 {
        self.start
    }
    /// Actual generated samples, excluding the audio buffer's silent tail.
    #[must_use]
    pub fn samples(&self) -> &[[f64; 2]] {
        &self.samples[..self.frames]
    }
}
/// Single real-time producer; destruction belongs to stream teardown, not fill.
pub struct ObservationProducer {
    queue: rtrb::Producer<ObservationPacket>,
    cursor: Arc<AtomicU64>,
}
/// Single non-real-time consumer; polling bounds are owned by the application.
pub struct ObservationConsumer {
    queue: rtrb::Consumer<ObservationPacket>,
    cursor: Arc<AtomicU64>,
}
fn packet_capacity(rate: SampleRate) -> usize {
    // Ceil(rate * 250 ms / packet), including at least one packet at low rates.
    usize::try_from(
        u64::from(rate.hz())
            .div_ceil(4)
            .div_ceil(OBSERVATION_FRAMES as u64),
    )
    .unwrap_or(usize::MAX)
}
/// Exact owned transport capacities and metadata on supported native targets.
/// Includes both endpoints, ring metadata, cursor Arc payload/refcounts, and two
/// working packet copies (producer construction and consumer processing).
/// Allocator bookkeeping and process RSS are outside this capacity bound.
#[must_use]
pub fn transport_capacity_bytes(rate: SampleRate) -> usize {
    packet_capacity(rate)
        .checked_mul(size_of::<ObservationPacket>())
        .and_then(|n| n.checked_add(size_of::<rtrb::RingBuffer<ObservationPacket>>()))
        .and_then(|n| {
            n.checked_add(size_of::<ObservationProducer>() + size_of::<ObservationConsumer>())
        })
        .and_then(|n| n.checked_add(size_of::<AtomicU64>() + 2 * size_of::<usize>()))
        .and_then(|n| n.checked_add(2 * size_of::<ObservationPacket>()))
        .unwrap_or(usize::MAX)
}
/// Allocate a 250 ms target rounded up to whole fixed packets before playback.
/// This uses rtrb 0.4.0's immediate-return safe SPSC interface. Supported native
/// targets provide lock-free pointer and 64-bit atomics; no retry loop is used.
#[must_use]
pub fn observation_channel(rate: SampleRate) -> (ObservationProducer, ObservationConsumer) {
    let (producer, consumer) = rtrb::RingBuffer::new(packet_capacity(rate));
    let cursor = Arc::new(AtomicU64::new(0));
    (
        ObservationProducer {
            queue: producer,
            cursor: Arc::clone(&cursor),
        },
        ObservationConsumer {
            queue: consumer,
            cursor,
        },
    )
}
impl ObservationProducer {
    #[cfg(test)]
    pub(crate) fn queue_full(&self) -> bool {
        self.queue.is_full()
    }
    /// Observe a real rendered span once, without allocation, locks, waits or
    /// retry. Returns false for zero/oversized/short input or unavailable queue.
    /// The latest-generated cursor advances even when visual data is dropped.
    /// Samples represent generation, including a later device-cancellation race.
    pub fn publish(&mut self, span: RenderedSpan, samples: &[[f64; 2]]) -> bool {
        let frames = span.frames();
        if frames == 0
            || frames > OBSERVATION_FRAMES
            || frames > samples.len()
            || span.start() < self.cursor.load(Ordering::Relaxed)
        {
            return false;
        }
        self.cursor.store(span.end(), Ordering::Relaxed);
        if self.queue.is_abandoned() || self.queue.is_full() {
            return false;
        }
        let mut packet = ObservationPacket {
            start: span.start(),
            frames,
            samples: [[0.0; 2]; OBSERVATION_FRAMES],
        };
        packet.samples[..frames].copy_from_slice(&samples[..frames]);
        self.queue.push(packet).is_ok()
    }
}
impl ObservationConsumer {
    /// Pop once immediately; no waiting and no producer backpressure.
    pub fn pop(&mut self) -> Option<ObservationPacket> {
        self.queue.pop().ok()
    }
    /// Latest exclusive generated source end, independent of queue coverage and
    /// terminal outcomes. A relaxed atomic is sufficient for this copied fact;
    /// packet publication itself uses rtrb's acquire/release synchronization.
    #[must_use]
    pub fn latest_generated(&self) -> u64 {
        self.cursor.load(Ordering::Relaxed)
    }
    /// Fixed maximum packets to consider for a bounded consumer poll.
    #[must_use]
    pub fn packet_capacity(&self) -> usize {
        self.queue.buffer().capacity()
    }
    /// Actual capacity accounting, including working copies and cursor storage.
    #[must_use]
    pub fn capacity_bytes(&self) -> usize {
        self.packet_capacity() * size_of::<ObservationPacket>()
            + size_of::<rtrb::RingBuffer<ObservationPacket>>()
            + size_of::<ObservationProducer>()
            + size_of::<Self>()
            + size_of::<AtomicU64>()
            + 2 * size_of::<usize>()
            + 2 * size_of::<ObservationPacket>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auditory_core::Stimulus;
    fn rate(hz: u32) -> SampleRate {
        SampleRate::new(hz).unwrap()
    }
    fn renderer(frames: u32, hz: u32) -> auditory_core::Renderer {
        Stimulus {
            duration_seconds: f64::from(frames) / f64::from(hz),
            fade_seconds: 0.0,
            components: vec![],
        }
        .session(rate(hz))
        .unwrap()
        .into_renderer()
    }
    #[test]
    fn every_supported_rate_fits_transport_and_actual_allocations_match_capacity_accounting() {
        for hz in 1..=SampleRate::MAX_HZ {
            let count = packet_capacity(rate(hz));
            assert!((1..=188).contains(&count));
            assert!(count as u64 * OBSERVATION_FRAMES as u64 * 4 >= u64::from(hz));
            assert!(transport_capacity_bytes(rate(hz)) < 3 * 1024 * 1024);
        }
        for hz in [1, 4095, 4096, 4097, 44100, 48000, 96000, 767999, 768000] {
            let (_, consumer) = observation_channel(rate(hz));
            assert_eq!(consumer.packet_capacity(), packet_capacity(rate(hz)));
            assert_eq!(
                consumer.capacity_bytes(),
                transport_capacity_bytes(rate(hz))
            );
        }
        assert!(!std::mem::needs_drop::<ObservationPacket>());
        let rate = rate(768000);
        let measured = allocation_counter::measure(|| {
            std::hint::black_box(observation_channel(rate));
        });
        let heap = packet_capacity(rate) * size_of::<ObservationPacket>()
            + size_of::<rtrb::RingBuffer<ObservationPacket>>()
            + size_of::<AtomicU64>()
            + 2 * size_of::<usize>();
        assert_eq!(measured.bytes_total, heap as u64);
        println!(
            "OBSERVATION_CAPACITY,rate=768000,packet_bytes={},packets={},heap_bytes={},total_with_working_copies={}",
            size_of::<ObservationPacket>(),
            packet_capacity(rate),
            heap,
            transport_capacity_bytes(rate)
        );
    }
    #[test]
    fn full_unconsumed_disconnected_and_partial_packets_preserve_cursor_and_source_gaps() {
        let (mut producer, mut consumer) = observation_channel(rate(8000));
        let mut renderer = renderer(9001, 8000);
        let mut samples = [[0.0; 2]; 1024];
        for i in 0..5 {
            let span = renderer.render(&mut samples);
            assert_eq!(producer.publish(span, &samples), i < 2);
            assert_eq!(consumer.latest_generated(), (i + 1) * 1024);
        }
        let first = consumer.pop().unwrap();
        let second = consumer.pop().unwrap();
        assert_eq!((first.start(), first.samples().len()), (0, 1024));
        assert_eq!((second.start(), second.samples().len()), (1024, 1024));
        assert!(consumer.pop().is_none());
        let span = renderer.render(&mut samples);
        assert!(producer.publish(span, &samples));
        assert_eq!(consumer.pop().unwrap().start(), 5120);
        while !renderer.is_complete() {
            let span = renderer.render(&mut samples);
            producer.publish(span, &samples);
            let packet = consumer.pop().unwrap();
            if span.end() == 9001 {
                assert_eq!(packet.samples().len(), 809);
            }
        }
        let span = renderer.render(&mut samples);
        assert_eq!(span.frames(), 0);
        assert!(!producer.publish(span, &samples));
        let cursor = Arc::clone(&producer.cursor);
        drop(consumer);
        let mut another = renderer_for_gap();
        let span = another.render(&mut samples);
        // A producer cannot regress its independently copied cursor.
        assert!(!producer.publish(span, &samples));
        assert_eq!(cursor.load(Ordering::Relaxed), 9001);
    }
    fn renderer_for_gap() -> auditory_core::Renderer {
        renderer(20000, 8000)
    }
    #[test]
    fn a_disconnected_consumer_drops_immediately_but_the_cursor_still_advances() {
        let (mut producer, consumer) = observation_channel(rate(768000));
        let cursor = Arc::clone(&producer.cursor);
        drop(consumer);
        let mut renderer = renderer(1025, 768000);
        let mut samples = [[0.0; 2]; 1024];
        let span = renderer.render(&mut samples);
        let count = allocation_counter::measure(|| assert!(!producer.publish(span, &samples)));
        assert_eq!((count.count_total, count.bytes_total), (0, 0));
        assert_eq!(cursor.load(Ordering::Relaxed), 1024);
        let span = renderer.render(&mut samples);
        assert!(!producer.publish(span, &samples));
        assert_eq!(cursor.load(Ordering::Relaxed), 1025);
    }
    #[test]
    fn synchronized_single_producer_consumer_preserves_published_order_without_backpressure() {
        let (mut producer, mut consumer) = observation_channel(rate(48000));
        let worker = std::thread::spawn(move || {
            let mut renderer = renderer(100000, 48000);
            let mut samples = [[0.0; 2]; 1024];
            while !renderer.is_complete() {
                let span = renderer.render(&mut samples);
                producer.publish(span, &samples);
            }
        });
        let mut end = 0;
        while !worker.is_finished() {
            if let Some(packet) = consumer.pop() {
                assert!(packet.start() >= end);
                end = packet.start() + packet.samples().len() as u64;
            }
        }
        worker.join().unwrap();
        for _ in 0..consumer.packet_capacity() {
            if let Some(packet) = consumer.pop() {
                assert!(packet.start() >= end);
                end = packet.start() + packet.samples().len() as u64;
            }
        }
        assert_eq!(consumer.latest_generated(), 100000);
        assert!(end <= 100000);
        assert!(consumer.pop().is_none());
    }
}
