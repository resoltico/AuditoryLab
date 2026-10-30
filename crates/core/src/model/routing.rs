//! Digital routing vocabulary and channel membership.
/// Digital channels receiving a component; acoustic ear delivery is not verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelRouting {
    /// The left digital channel.
    Left,
    /// The right digital channel.
    Right,
    /// Identical signal in both digital channels.
    Both,
}

/// One digital channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Channel zero of each stereo frame.
    Left,
    /// Channel one of each stereo frame.
    Right,
}

impl ChannelRouting {
    pub(crate) const fn includes(self, channel: Channel) -> bool {
        matches!(
            (self, channel),
            (Self::Both, _) | (Self::Left, Channel::Left) | (Self::Right, Channel::Right)
        )
    }
}

crate::vocabulary::vocabulary! { ChannelRouting,"channel_routing" {
    Both => ("both","Both channels","both channels"),
    Left => ("left","Left channel","left channel"),
    Right => ("right","Right channel","right channel"),
}}

crate::vocabulary::vocabulary! { Channel,"channel" {
    Left => (crate::ChannelRouting::Left.key(),
        crate::ChannelRouting::Left.label(),crate::ChannelRouting::Left.phrase()),
    Right => (crate::ChannelRouting::Right.key(),
        crate::ChannelRouting::Right.label(),crate::ChannelRouting::Right.phrase()),
}}
