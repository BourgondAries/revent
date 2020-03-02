//! Synchronous event system.
//!
//! # What is an event system? #
//!
//! An event system is a set of slots which contain objects. A signal is emitted on a slot, which
//! will call each object in the slot. Invoked objects can then send more signals to different
//! slots.
//!
//! # Synchronous #
//!
//! Revent's events are synchronous, meaning that emitting an event will immediately process all
//! handlers in a slot. Once the function call returns, it is guaranteed that all listeners have
//! been called.
//!
//! # Example #
//!
//! ```
//! use revent::{Manager, Named, Node, Null, Slot, Subscriber};
//! use std::{cell::RefCell, rc::Rc};
//!
//! trait BasicSignal {}
//!
//! struct Hub {
//!     basic_slot: Slot<dyn BasicSignal>,
//!     mng: Rc<RefCell<Manager>>,
//! }
//! impl Hub {
//!     fn new() -> Self {
//!         let mng = Rc::new(RefCell::new(Manager::default()));
//!         Self {
//!             basic_slot: Slot::new("basic_slot", mng.clone()),
//!             mng,
//!         }
//!     }
//! }
//! impl Node for Hub {
//!     fn manager(&self) -> &Rc<RefCell<Manager>> {
//!         &self.mng
//!     }
//! }
//!
//! // ---
//!
//! struct MySubscriber;
//! impl Subscriber<Hub> for MySubscriber {
//!     type Input = ();
//!     type Node = Null;
//!
//!     fn create(_: Self::Input, _: Self::Node) -> Self {
//!         Self
//!     }
//!
//!     fn register(hub: &mut Hub, item: Rc<RefCell<Self>>) {
//!         hub.basic_slot.register(item);
//!     }
//! }
//! impl Named for MySubscriber {
//!     const NAME: &'static str = "MySubscriber";
//! }
//! impl BasicSignal for MySubscriber {}
//!
//! // ---
//!
//! let mut hub = Hub::new();
//! let item = hub.subscribe::<MySubscriber>(());
//! hub.basic_slot.emit(|x| {
//!     println!("Called for each subscriber");
//! });
//! hub.unsubscribe(&item);
//! ```
//!
//! # Mutable cycles #
//!
//! Revent performs cycle detection in [subscribe](crate::Node::subscribe) and ensures that no
//! system exists in which we can create double mutable borrows.
#![deny(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unused_import_braces,
    unused_qualifications
)]

mod mng;
mod single;
mod slot;
mod traits;
pub(crate) use self::mng::Mode;
pub use self::{
    mng::{Grapher, Manager},
    single::Single,
    slot::Slot,
    traits::{Named, Node, Subscriber},
};

use std::{cell::RefCell, rc::Rc};

thread_local! {
    static STACK: RefCell<Vec<(Mode, Rc<RefCell<Manager>>)>> = RefCell::new(Vec::new());
}

fn assert_active_manager(manager: &Rc<RefCell<Manager>>) {
    STACK.with(|x| {
        assert!(
            Rc::ptr_eq(
                &x.borrow()
                    .last()
                    .expect("revent signal modification outside of Node context")
                    .1,
                manager
            ),
            "revent manager is different"
        );
    });
}

/// Null `Node` value for subscribers.
///
/// Use this when you want a subscriber that has no further signals to anything else.
/// ```
/// use revent::{Manager, Named, Null, Slot, Node, Subscriber};
/// use std::{cell::RefCell, rc::Rc};
///
/// trait BasicSignal {}
///
/// struct Hub {
///     basic_signal: Slot<dyn BasicSignal>,
///     mng: Rc<RefCell<Manager>>,
/// }
/// impl Hub {
///     fn new() -> Self {
///         let mng = Rc::new(RefCell::new(Manager::default()));
///         Self {
///             basic_signal: Slot::new("basic_signal", mng.clone()),
///             mng,
///         }
///     }
/// }
/// impl Node for Hub {
///     fn manager(&self) -> &Rc<RefCell<Manager>> {
///         &self.mng
///     }
/// }
///
/// // ---
///
/// struct MySubscriber;
/// impl Subscriber<Hub> for MySubscriber {
///     type Input = ();
///     type Node = Null;
///
///     fn create(_: Self::Input, _: Self::Node) -> Self {
///         Self
///     }
///
///     fn register(hub: &mut Hub, item: Rc<RefCell<Self>>) {
///         hub.basic_signal.register(item);
///     }
/// }
/// impl Named for MySubscriber {
///     const NAME: &'static str = "MySubscriber";
/// }
/// impl BasicSignal for MySubscriber {}
/// ```
pub struct Null;

impl<T> From<&T> for Null {
    fn from(_: &T) -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use crate::{Manager, Named, Node, Slot, Subscriber};
    use std::{cell::RefCell, rc::Rc};

    #[quickcheck_macros::quickcheck]
    fn basic(value: usize) {
        trait BasicSignal {}

        struct Hub {
            basic_signal: Slot<dyn BasicSignal>,
            mng: Rc<RefCell<Manager>>,
        }
        impl Hub {
            fn new() -> Self {
                let mng = Rc::new(RefCell::new(Manager::default()));
                Self {
                    basic_signal: Slot::new("basic_signal", mng.clone()),

                    mng,
                }
            }
        }
        impl Node for Hub {
            fn manager(&self) -> &Rc<RefCell<Manager>> {
                &self.mng
            }
        }

        // ---

        struct MySubscriberNode;
        impl From<&Hub> for MySubscriberNode {
            fn from(_: &Hub) -> Self {
                Self
            }
        }
        struct MySubscriber;
        impl Subscriber<Hub> for MySubscriber {
            type Input = ();
            type Node = MySubscriberNode;

            fn create(_: Self::Input, _: Self::Node) -> Self {
                Self
            }

            fn register(hub: &mut Hub, item: Rc<RefCell<Self>>) {
                hub.basic_signal.register(item);
            }
        }
        impl Named for MySubscriber {
            const NAME: &'static str = "MySubscriber";
        }
        impl BasicSignal for MySubscriber {}

        // ---

        let mut hub = Hub::new();

        for _ in 0..value {
            hub.subscribe::<MySubscriber>(());
        }

        let mut count = 0;

        hub.basic_signal.emit(|_| {
            count += 1;
        });

        assert_eq!(value, count);
    }

    #[test]
    #[should_panic(
        expected = "revent found a recursion during subscription: [MySubscriber]basic_signal -> basic_signal"
    )]
    fn self_subscribing() {
        trait BasicSignal {}

        struct Hub {
            basic_signal: Slot<dyn BasicSignal>,
            mng: Rc<RefCell<Manager>>,
        }
        impl Hub {
            fn new() -> Self {
                let mng = Rc::new(RefCell::new(Manager::default()));
                Self {
                    basic_signal: Slot::new("basic_signal", mng.clone()),
                    mng,
                }
            }
        }
        impl Node for Hub {
            fn manager(&self) -> &Rc<RefCell<Manager>> {
                &self.mng
            }
        }

        // ---

        struct MySubscriberNode;
        impl From<&Hub> for MySubscriberNode {
            fn from(hub: &Hub) -> Self {
                let _ = hub.basic_signal.clone();
                Self
            }
        }
        struct MySubscriber;
        impl Subscriber<Hub> for MySubscriber {
            type Input = ();
            type Node = MySubscriberNode;
            fn create(_: Self::Input, _: Self::Node) -> Self {
                Self
            }
            fn register(hub: &mut Hub, item: Rc<RefCell<Self>>) {
                hub.basic_signal.register(item);
            }
        }
        impl Named for MySubscriber {
            const NAME: &'static str = "MySubscriber";
        }
        impl BasicSignal for MySubscriber {}

        // ---

        let mut hub = Hub::new();

        hub.subscribe::<MySubscriber>(());
    }

    #[test]
    #[should_panic(
        expected = "revent found a recursion during subscription: [MySubscriber]basic_signal -> [OtherSubscriber]other_signal -> basic_signal"
    )]
    fn transitive_self_subscription() {
        trait BasicSignal {}
        trait OtherSignal {}

        struct Hub {
            basic_signal: Slot<dyn BasicSignal>,
            other_signal: Slot<dyn OtherSignal>,
            mng: Rc<RefCell<Manager>>,
        }
        impl Hub {
            fn new() -> Self {
                let mng = Rc::new(RefCell::new(Manager::default()));
                Self {
                    basic_signal: Slot::new("basic_signal", mng.clone()),
                    other_signal: Slot::new("other_signal", mng.clone()),
                    mng,
                }
            }
        }
        impl Node for Hub {
            fn manager(&self) -> &Rc<RefCell<Manager>> {
                &self.mng
            }
        }

        // ---

        struct MySubscriberNode;
        impl From<&Hub> for MySubscriberNode {
            fn from(hub: &Hub) -> Self {
                let _ = hub.other_signal.clone();
                Self
            }
        }
        struct MySubscriber;
        impl Subscriber<Hub> for MySubscriber {
            type Input = ();
            type Node = MySubscriberNode;
            fn create(_: Self::Input, _: Self::Node) -> Self {
                Self
            }
            fn register(hub: &mut Hub, item: Rc<RefCell<Self>>) {
                hub.basic_signal.register(item);
            }
        }
        impl Named for MySubscriber {
            const NAME: &'static str = "MySubscriber";
        }
        impl BasicSignal for MySubscriber {}

        // ---

        struct OtherSubscriberNode;
        impl From<&Hub> for OtherSubscriberNode {
            fn from(hub: &Hub) -> Self {
                let _ = hub.basic_signal.clone();
                Self
            }
        }
        struct OtherSubscriber;
        impl Subscriber<Hub> for OtherSubscriber {
            type Input = ();
            type Node = OtherSubscriberNode;
            fn create(_: Self::Input, _: Self::Node) -> Self {
                Self
            }
            fn register(hub: &mut Hub, item: Rc<RefCell<Self>>) {
                hub.other_signal.register(item);
            }
        }
        impl Named for OtherSubscriber {
            const NAME: &'static str = "OtherSubscriber";
        }
        impl OtherSignal for OtherSubscriber {}

        // ---

        let mut hub = Hub::new();

        hub.subscribe::<MySubscriber>(());
        hub.subscribe::<OtherSubscriber>(());
    }

    #[quickcheck_macros::quickcheck]
    fn register_and_unsubscribe(subscribes: usize) {
        trait BasicSignal {}

        struct Hub {
            basic_signal: Slot<dyn BasicSignal>,
            mng: Rc<RefCell<Manager>>,
        }
        impl Hub {
            fn new() -> Self {
                let mng = Rc::new(RefCell::new(Manager::default()));
                Self {
                    basic_signal: Slot::new("basic_signal", mng.clone()),
                    mng,
                }
            }
        }
        impl Node for Hub {
            fn manager(&self) -> &Rc<RefCell<Manager>> {
                &self.mng
            }
        }

        // ---

        struct MySubscriberNode;
        impl From<&Hub> for MySubscriberNode {
            fn from(_: &Hub) -> Self {
                Self
            }
        }
        struct MySubscriber;
        impl Subscriber<Hub> for MySubscriber {
            type Input = ();
            type Node = MySubscriberNode;
            fn create(_: Self::Input, _: Self::Node) -> Self {
                Self
            }
            fn register(hub: &mut Hub, item: Rc<RefCell<Self>>) {
                hub.basic_signal.register(item);
            }
        }
        impl Named for MySubscriber {
            const NAME: &'static str = "MySubscriber";
        }
        impl BasicSignal for MySubscriber {}

        // ---

        let mut hub = Hub::new();

        let mut items = Vec::with_capacity(subscribes);
        for _ in 0..subscribes {
            items.push(hub.subscribe::<MySubscriber>(()));
        }

        {
            let mut count = 0;
            hub.basic_signal.emit(|_| {
                count += 1;
            });
            assert_eq!(subscribes, count);
        }

        for item in items.drain(..) {
            hub.unsubscribe::<MySubscriber>(&item);
        }

        {
            let mut count = 0;
            hub.basic_signal.emit(|_| {
                count += 1;
            });
            assert_eq!(0, count);
        }
    }

    #[test]
    #[should_panic(expected = "unable to unsubscribe non-subscribed item")]
    fn double_unsubscribe() {
        trait BasicSignal {}

        struct Hub {
            basic_signal: Slot<dyn BasicSignal>,
            mng: Rc<RefCell<Manager>>,
        }
        impl Hub {
            fn new() -> Self {
                let mng = Rc::new(RefCell::new(Manager::default()));
                Self {
                    basic_signal: Slot::new("basic_signal", mng.clone()),
                    mng,
                }
            }
        }
        impl Node for Hub {
            fn manager(&self) -> &Rc<RefCell<Manager>> {
                &self.mng
            }
        }

        // ---

        struct MySubscriberNode;
        impl From<&Hub> for MySubscriberNode {
            fn from(_: &Hub) -> Self {
                Self
            }
        }
        struct MySubscriber;
        impl Subscriber<Hub> for MySubscriber {
            type Input = ();
            type Node = MySubscriberNode;
            fn create(_: Self::Input, _: Self::Node) -> Self {
                Self
            }
            fn register(hub: &mut Hub, item: Rc<RefCell<Self>>) {
                hub.basic_signal.register(item);
            }
        }
        impl Named for MySubscriber {
            const NAME: &'static str = "MySubscriber";
        }
        impl BasicSignal for MySubscriber {}

        // ---

        let mut hub = Hub::new();
        let item = hub.subscribe::<MySubscriber>(());
        hub.unsubscribe(&item);
        hub.unsubscribe(&item);
    }

    #[test]
    fn double_unsubscribe_deaf_node() {
        struct Hub {
            mng: Rc<RefCell<Manager>>,
        }
        impl Hub {
            fn new() -> Self {
                let mng = Rc::new(RefCell::new(Manager::default()));
                Self { mng }
            }
        }
        impl Node for Hub {
            fn manager(&self) -> &Rc<RefCell<Manager>> {
                &self.mng
            }
        }

        // ---

        struct MySubscriberNode;
        impl From<&Hub> for MySubscriberNode {
            fn from(_: &Hub) -> Self {
                Self
            }
        }
        struct MySubscriber;
        impl Subscriber<Hub> for MySubscriber {
            type Input = ();
            type Node = MySubscriberNode;
            fn create(_: Self::Input, _: Self::Node) -> Self {
                Self
            }
            fn register(_: &mut Hub, _: Rc<RefCell<Self>>) {}
        }
        impl Named for MySubscriber {
            const NAME: &'static str = "MySubscriber";
        }

        // ---

        let mut hub = Hub::new();
        let item = hub.subscribe::<MySubscriber>(());
        hub.unsubscribe(&item);
        hub.unsubscribe(&item);
    }
}
