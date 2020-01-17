//! Event broker library for Rust.
//!
//! Implements a synchronous transitive event broker that does not violate mutability constraints.
//! It does so by performing DAG traversals to ensure that no signal chains are able
//! to form a loop.
//!
//! # What is an event broker? #
//!
//! An event broker is a bag of objects and a bunch of "signals". Each object decides which signals to listen to.
//!
//! Each object (also called subscriber) is notified when its subscribed signal is [emit](crate::Topic::emit)ted.
//! A subscriber may during notification processing emit yet another signal into the broker, and so
//! on.
//!
//! ```
//! use revent::{hub, Shared, Subscriber};
//!
//! // Construct a trait for some type of event.
//! trait MyEvent {
//!     fn my_event(&mut self);
//! }
//!
//! // Create an event hub.
//! hub! {
//!     Hub {
//!         event: dyn MyEvent,
//!     }
//! }
//!
//! // Construct a hub object.
//! let hub = Hub::default();
//!
//! // Implement a subscriber to some event.
//! struct X;
//! impl MyEvent for X {
//!     fn my_event(&mut self) {
//!         println!("Hello world");
//!     }
//! }
//! impl Subscriber<Hub> for X {
//!     type Input = ();
//!     fn build(_: Hub, input: Self::Input) -> Self {
//!         Self
//!     }
//!     fn subscribe(hub: &Hub, shared: Shared<Self>) {
//!         hub.event.subscribe(shared);
//!     }
//! }
//!
//! // Create an instance of X in the hub.
//! hub.subscribe::<X>(());
//!
//! // Now emit an event into the topic.
//! hub.event.emit(|x| {
//!     x.my_event();
//! });
//! ```
#![deny(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unused_import_braces,
    unused_qualifications
)]
#![feature(coerce_unsized, drain_filter, unsize)]

mod mng;
mod shared;
mod topic;
pub use mng::Manager;
pub use shared::Shared;
pub use topic::Topic;

/// Generate an event hub and its associated boilerplate code.
///
/// ```
/// use revent::hub;
///
/// pub trait MyTrait1 {}
/// pub trait MyTrait2 {}
///
/// hub! {
///     MyHub {
///         channel_name1: dyn MyTrait1,
///         channel_name2: dyn MyTrait2,
///     }
/// }
///
/// let my_hub = MyHub::default();
/// // or
/// let my_hub = MyHub::new();
///
/// my_hub.channel_name1.emit(|_| {
///     // Do something with each subscriber of channel_name1.
/// });
/// ```
///
/// The macro generates a struct of `MyHub` containing all topics. [Topic]s are public members of
/// the struct. In addition, [Default] is implemented as well as `new` and `subscribe`.
#[macro_export]
macro_rules! hub {
    ($hub:ident { $($channel:ident: $type:ty),*$(,)? }) => {
        /// Hub of events.
        ///
        /// Contains various [Topic]ics which can be emitted into or subscribed to.
        pub struct $hub {
            $(
                /// Channel for the given $type.
                pub $channel: $crate::Topic<$type>
            ),*,
            // TODO: When gensyms are supported make this symbol a gensym.
            #[doc(hidden)]
            pub _manager: ::std::rc::Rc<::std::cell::RefCell<$crate::Manager>>,
        }

        impl Default for $hub {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $hub {
            /// Create a new hub.
            pub fn new() -> Self {
                let mng = ::std::rc::Rc::new(::std::cell::RefCell::new($crate::Manager::default()));
                Self {
                    $($channel: $crate::Topic::new(stringify!($channel), &mng)),*,
                    _manager: mng,
                }
            }

            /// Insert a subscriber into the hub.
            pub fn subscribe<T: $crate::Subscriber<Self>>(&self, input: T::Input) {
                self.manager().borrow_mut().begin_construction();
                let hub = self.clone_deactivate();
                let shared = $crate::Shared(::std::rc::Rc::new(::std::cell::RefCell::new(T::build(hub, input))));
                T::subscribe(self, shared);
                self.manager().borrow_mut().end_construction();
            }

            #[doc(hidden)]
            fn clone_deactivate(&self) -> Self {
                Self {
                    $($channel: self.$channel.clone_deactivate()),*,
                    _manager: self.manager().clone(),
                }
            }

            #[doc(hidden)]
            pub fn manager(&self) -> ::std::rc::Rc<::std::cell::RefCell<$crate::Manager>> {
                self._manager.clone()
            }
        }
    };
}

/// Subscriber to an event hub.
///
/// Is used by the `subscribe` function generated by the [hub](hub) macro.
pub trait Subscriber<T>
where
    Self: Sized,
{
    /// Input data to the build function.
    type Input;
    /// Build an object using any hub and arbitrary input.
    fn build(hub: T, input: Self::Input) -> Self;
    /// Subscribe to a specific hub.
    ///
    /// This function wraps the self object inside an opaque wrapper which can be used on
    /// [Topic::subscribe].
    fn subscribe(hub: &T, shared: Shared<Self>);
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn simple_listener() {
        pub trait Event {}

        hub! {
            Hub {
                event: dyn Event,
            }
        }

        let hub = Hub::default();

        struct X;
        impl Event for X {}
        impl Subscriber<Hub> for X {
            type Input = ();
            fn build(_: Hub, _: Self::Input) -> Self {
                Self
            }
            fn subscribe(hub: &Hub, shared: Shared<Self>) {
                hub.event.subscribe(shared);
            }
        }

        hub.subscribe::<X>(());

        let mut count = 0;
        hub.event.emit(|_| {
            count += 1;
        });
        assert_eq!(count, 1);
    }

    #[test]
    #[should_panic(expected = "Topic is not active: event2")]
    fn emit_on_non_activated_channel() {
        pub trait Event {
            fn event(&mut self);
        }

        hub! {
            Hub {
                event1: dyn Event,
                event2: dyn Event,
            }
        }

        let hub = Hub::default();

        struct X {
            hub: Hub,
        }
        impl Event for X {
            fn event(&mut self) {
                self.hub.event2.emit(|_| {});
            }
        }
        impl Subscriber<Hub> for X {
            type Input = ();
            fn build(hub: Hub, _: Self::Input) -> Self {
                Self { hub }
            }
            fn subscribe(hub: &Hub, shared: Shared<Self>) {
                hub.event1.subscribe(shared);
            }
        }

        hub.subscribe::<X>(());

        hub.event1.emit(|x| {
            x.event();
        });
    }

    #[test]
    #[should_panic(expected = "Recursion detected: [\"event\"]")]
    fn recursion_to_self() {
        pub trait Event {}

        hub! {
            Hub {
                event: dyn Event,
            }
        }

        let hub = Hub::default();

        struct X;
        impl Event for X {}
        impl Subscriber<Hub> for X {
            type Input = ();
            fn build(mut hub: Hub, _: Self::Input) -> Self {
                hub.event.activate();
                Self
            }
            fn subscribe(hub: &Hub, shared: Shared<Self>) {
                hub.event.subscribe(shared);
            }
        }

        hub.subscribe::<X>(());
    }

    #[test]
    #[should_panic(expected = "Recursion detected: [\"event1\", \"event2\"]")]
    fn transitive_recursion() {
        pub trait Event {}

        hub! {
            Hub {
                event1: dyn Event,
                event2: dyn Event,
            }
        }

        let hub = Hub::default();

        struct X;
        impl Event for X {}
        impl Subscriber<Hub> for X {
            type Input = ();
            fn build(mut hub: Hub, _: Self::Input) -> Self {
                hub.event1.activate();
                Self
            }
            fn subscribe(hub: &Hub, shared: Shared<Self>) {
                hub.event2.subscribe(shared);
            }
        }

        struct Y;
        impl Event for Y {}
        impl Subscriber<Hub> for Y {
            type Input = ();
            fn build(mut hub: Hub, _: Self::Input) -> Self {
                hub.event2.activate();
                Self
            }
            fn subscribe(hub: &Hub, shared: Shared<Self>) {
                hub.event1.subscribe(shared);
            }
        }

        hub.subscribe::<X>(());
        hub.subscribe::<Y>(());
    }

    #[test]
    fn no_subscription_is_dropped() {
        use std::{cell::Cell, rc::Rc};

        pub trait Event {}

        hub! {
            Hub {
                event: dyn Event,
            }
        }

        let hub = Hub::default();

        struct X {
            dropped: Rc<Cell<bool>>,
        }
        impl Event for X {}
        impl Subscriber<Hub> for X {
            type Input = Rc<Cell<bool>>;
            fn build(_: Hub, input: Self::Input) -> Self {
                Self { dropped: input }
            }
            fn subscribe(_: &Hub, _: Shared<Self>) {}
        }
        impl Drop for X {
            fn drop(&mut self) {
                self.dropped.set(true);
            }
        }

        let dropped: Rc<Cell<bool>> = Default::default();
        assert_eq!(dropped.get(), false);
        hub.subscribe::<X>(dropped.clone());
        assert_eq!(dropped.get(), true);
    }
}
