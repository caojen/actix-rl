use std::fmt::{Display, Formatter};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Error {
    /// [RateLimited] indicates that the limit has been reached
    /// and returns the time when the limit will be lifted.
    RateLimited(Option<DateTime<Utc>>),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::RateLimited(until) => if let Some(until) = until {
                write!(f, "rate limited, until {}", until.timestamp())
            } else {
                write!(f, "rate limited")
            }
        }
    }
}

impl std::error::Error for Error {}
