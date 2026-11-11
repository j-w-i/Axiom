//! Compile-time binding of method names to parameter and result types.

use serde::{de::DeserializeOwned, Serialize};

/// A request/response method. Implemented by zero-sized marker types.
pub trait Method {
    /// Wire name, `domain/verb` (e.g. `fs/readFile`).
    const NAME: &'static str;
    type Params: Serialize + DeserializeOwned + Send + 'static;
    type Result: Serialize + DeserializeOwned + Send + 'static;
}

/// A one-way notification. Implemented by zero-sized marker types.
pub trait Notification {
    /// Wire name. Names starting with `$/` are protocol-level.
    const NAME: &'static str;
    type Params: Serialize + DeserializeOwned + Send + 'static;
}

/// Declares a [`Method`] marker type.
#[macro_export]
macro_rules! method {
    ($(#[$m:meta])* $ty:ident, $name:literal, $params:ty => $result:ty) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy)]
        pub enum $ty {}
        impl $crate::method::Method for $ty {
            const NAME: &'static str = $name;
            type Params = $params;
            type Result = $result;
        }
    };
}

/// Declares a [`Notification`] marker type.
#[macro_export]
macro_rules! notification {
    ($(#[$m:meta])* $ty:ident, $name:literal, $params:ty) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy)]
        pub enum $ty {}
        impl $crate::method::Notification for $ty {
            const NAME: &'static str = $name;
            type Params = $params;
        }
    };
}
