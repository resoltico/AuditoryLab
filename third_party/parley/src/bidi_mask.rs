// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Local bidi-class set membership, independent of ICU numeric encodings.
use icu_properties::props::BidiClass;

pub(super) const fn mask(t: BidiClass) -> u32 {
    // Local set membership: independent of ICU4C numeric property values.
    match t {
        BidiClass::ArabicLetter => 1 << 0,
        BidiClass::ArabicNumber => 1 << 1,
        BidiClass::BoundaryNeutral => 1 << 2,
        BidiClass::CommonSeparator => 1 << 3,
        BidiClass::EuropeanNumber => 1 << 4,
        BidiClass::EuropeanSeparator => 1 << 5,
        BidiClass::EuropeanTerminator => 1 << 6,
        BidiClass::FirstStrongIsolate => 1 << 7,
        BidiClass::LeftToRight => 1 << 8,
        BidiClass::LeftToRightEmbedding => 1 << 9,
        BidiClass::LeftToRightIsolate => 1 << 10,
        BidiClass::LeftToRightOverride => 1 << 11,
        BidiClass::NonspacingMark => 1 << 12,
        BidiClass::OtherNeutral => 1 << 13,
        BidiClass::ParagraphSeparator => 1 << 14,
        BidiClass::PopDirectionalFormat => 1 << 15,
        BidiClass::PopDirectionalIsolate => 1 << 16,
        BidiClass::RightToLeft => 1 << 17,
        BidiClass::RightToLeftEmbedding => 1 << 18,
        BidiClass::RightToLeftIsolate => 1 << 19,
        BidiClass::RightToLeftOverride => 1 << 20,
        BidiClass::SegmentSeparator => 1 << 21,
        BidiClass::WhiteSpace => 1 << 22,
        _ => 0,
    }
}
