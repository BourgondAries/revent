//! Synchronous event system.
//!
//! # What is an event system #
//!
//! An event system is a set of signals connected to a bunch of objects. When a signal is emitted,
//! the objects subscribing to said signal will have their handlers invoked to perform some useful
//! processing.
//!
//! ## Synchronous? ##
//!
//! Revent is synchonous, meaning that calling `emit` will immediately call all subscribers. This
//! also means that subscribers can return complex types with lifetimes referring to themselves.
//! Event handlers can also emit further events synchronously.
//!
//! # Example #
//!
//! ```
//! use revent::{hub, node, Subscriber};
//!
//! // Here is a simple trait for a signal. All signals are traits.
//! trait A {
//!     fn function(&mut self);
//! }
//!
//! // Create a new top-level hub, this contains all signals.
//! hub! {
//!     X {
//!         signal_1: A,
//!     }
//! }
//!
//! // Make `MyHandler` subscribe to `channel`.
//! node! {
//!     X {
//!         signal_1: A,
//!     } => Node(MyHandler) {
//!     }
//! }
//!
//! // Create the `MyHandler` struct.
//! struct MyHandler;
//! impl A for MyHandler {
//!     fn function(&mut self) {
//!         println!("Hello world");
//!     }
//! }
//!
//! // Describe how to build an instance of `MyHandler`.
//! impl Subscriber for MyHandler {
//!     type Input = ();
//!     fn build(_node: Self::Node, _input: Self::Input) -> Self {
//!         Self
//!     }
//! }
//!
//! // Create a new root hub instance.
//! let mut x = X::new();
//!
//! // Add an instance of `MyHandler`.
//! let input = ();
//! x.subscribe::<MyHandler>(input);
//!
//! // Emit an event on the `signal_1` channel.
//! x.signal_1.emit(|subscriber| {
//!     subscriber.function();
//! });
//! ```
//!
//! # Nested emitting #
//!
//! To allow for nested emitting we specify which signals we wish to be able to emit to in our
//! internal node.
//!
//! ```
//! use revent::{hub, node, Subscriber};
//!
//! trait A {
//!     fn function_a(&mut self);
//! }
//!
//! trait B {
//!     fn function_b(&mut self);
//! }
//!
//! hub! {
//!     X {
//!         signal_1: A,
//!         signal_2: B,
//!     }
//! }
//!
//! node! {
//!     X {
//!         signal_1: A,
//!     } => Node(MyHandler) {
//!         signal_2: B,
//!         // Node holds `signal_2` and is able to emit into this.
//!     }
//! }
//!
//! struct MyHandler;
//! impl A for MyHandler {
//!     fn function_a(&mut self) { }
//! }
//!
//! // Describe how to build an instance of `MyHandler`.
//! impl Subscriber for MyHandler {
//!     type Input = ();
//!     fn build(mut node: Self::Node, _input: Self::Input) -> Self {
//!         node.signal_2.emit(|subscriber| {
//!             subscriber.function_b();
//!         });
//!         Self
//!     }
//! }
//! ```
//!
//! # Mutable borrowing #
//!
//! It's possible to put a single object in two or more [Signal]s. If one signal is able to emit
//! into another signal then we may get a double-mutable borrow.
//!
//! Revent avoids the possibility of mutable borrows at emit-time by performing a graph cycle search
//! every time a type subscribes. The following code panics at the subscribe stage giving us a
//! useful error message about how the cycle is formed.
//!
//! The following prints `[AToBHandler]a -> [BToAHandler]b -> a`, meaning that `AToBHandler`
//! listens to `a` and emits into `b` (which `BToAHandler` listens to), which then again emits into
//! `a`, thus a cycle is formed that can cause a double mutable borrow.
//!
//! ```should_panic
//! use revent::{hub, node, Subscriber};
//!
//! pub trait A {
//!     fn a(&mut self);
//! }
//!
//! pub trait B {
//!     fn b(&mut self);
//! }
//!
//! hub! {
//!     X {
//!         a: A,
//!         b: B,
//!     }
//! }
//!
//! node! {
//!     X {
//!         a: A,
//!     } => AToB(AToBHandler) {
//!         b: B,
//!     }
//! }
//!
//! struct AToBHandler {
//!     node: AToB,
//! }
//!
//! impl A for AToBHandler {
//!     fn a(&mut self) {
//!         self.node.b.emit(|x| {
//!             x.b();
//!         });
//!     }
//! }
//!
//! impl Subscriber for AToBHandler {
//!     type Input = ();
//!     fn build(node: Self::Node, _: Self::Input) -> Self {
//!         Self { node }
//!     }
//! }
//!
//! node! {
//!     X {
//!         b: B,
//!     } => BToA(BToAHandler) {
//!         a: A,
//!     }
//! }
//!
//! struct BToAHandler {
//!     node: BToA,
//! }
//!
//! impl B for BToAHandler {
//!     fn b(&mut self) {
//!         self.node.a.emit(|x| {
//!             x.a();
//!         });
//!     }
//! }
//!
//! impl Subscriber for BToAHandler {
//!     type Input = ();
//!     fn build(node: Self::Node, _: Self::Input) -> Self {
//!         Self { node }
//!     }
//! }
//!
//! let mut x = X::new();
//! x.subscribe::<AToBHandler>(());
//! x.subscribe::<BToAHandler>(());
//! ```
#![deny(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unused_import_braces,
    unused_qualifications
)]
#![feature(drain_filter)]

mod mng;
mod signal;
mod traits;
#[doc(hidden)]
pub use mng::Manager;
pub use signal::Signal;
pub use traits::{Nodified, Selfscriber, Subscriber};

/// Generate a top-level `hub`.
///
/// A hub is struct where all signals are defined. It is the "root" object for downstream [node]s.
///
/// The macro invocation
/// ```
/// use revent::hub;
///
/// trait X {}
/// trait Y {}
/// trait Z {}
///
/// hub! {
///     HubName {
///         signal_name_1: X,
///         signal_name_2: Y,
///         signal_name_3: Z,
///     }
/// }
/// ```
///
/// generates the code
///
/// ```ignore
/// struct HubName {
///     pub signal_name_1: revent::Signal<dyn X>,
///     pub signal_name_2: revent::Signal<dyn Y>,
///     pub signal_name_3: revent::Signal<dyn Z>,
/// }
///
/// impl HubName {
///     fn new() -> Self { ... }
///
///     pub fn subscribe<T>(&mut self, input: T::Input)
///     where
///         T: revent::Nodified + revent::Selfscriber<Self> + revent::Subscriber,
///         T::Node: for<'a> From<&'a Self>,
///     { ... }
/// }
///
/// impl Default for HubName {
///     fn default() -> Self { ... }
/// }
/// ```
#[macro_export]
macro_rules! hub {
    ($name:ident {
         $($channel:ident: $channel_type:path),*$(,)?
     }) => {
        $crate::node_internal! {
            hub $name {
                $($channel: $channel_type),*
            }
        }

        impl $name {
            pub fn new() -> Self {
                let manager = ::std::rc::Rc::new(::std::cell::RefCell::new($crate::Manager::default()));
                Self {
                    _private_revent_1_manager: manager.clone(),
                    $($channel: $crate::Signal::new(stringify!($channel), manager.clone())),*
                }
            }
        }

        impl ::std::default::Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    }
}

/// Generate an intermediate node in the signal chain.
///
/// The macro invocation
/// ```ignore
/// use revent::node;
///
/// node! {
///     HubName {
///         signal_name_1: X,
///         signal_name_2: Y,
///     } => MyNode(Handler) {
///         signal_name_3: Z,
///     }
/// }
/// ```
///
/// generates the code
///
/// ```ignore
/// struct MyNode {
///     pub signal_name_3: revent::Signal<dyn Z>,
/// }
///
/// impl MyNode {
///     pub fn subscribe<T>(&mut self, input: T::Input)
///     where
///         T: revent::Nodified + revent::Selfscriber<Self> + revent::Subscriber,
///         T::Node: for<'a> From<&'a Self>,
///     { ... }
/// }
///
/// impl From<&'_ HubName> for MyNode {
///     fn from(item: &HubName) -> Self { ... }
/// }
///
/// impl revent::Selfscriber<HubName> for MyHandler {
///     fn name() -> &'static str { ... }
///     fn selfscribe(holder: &HubName, item: Rc<RefCell<Self>>) { ... }
/// }
///
/// impl revent::Nodified for MyHandler {
///     type Node = MyNode;
/// }
/// ```
#[macro_export]
macro_rules! node {
    (
        $($source:path),+$(,)? {
            $($listen:ident: $listen_type:path),*$(,)?
        } => $hub:ident($on:path) {
            $($emit:ident: $emit_type:path),*$(,)?
        }
    ) => {
        $crate::node_internal! {
            hub $hub {
                $($emit: $emit_type),*
            }
        }

        $crate::node_internal! {
            from $hub, $($source),+ {
                $($emit: $emit_type),*
            }
        }

        $crate::node_internal! {
            selfscribe $on { $($source),* } {
                $($listen),*
            }
        }

        impl $crate::Nodified for $on {
            type Node = $hub;
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! node_internal {
    (hub $hub:ident {
         $($emit:ident: $emit_type:path),*
     }) => {
        pub struct $hub {
            _private_revent_1_manager: ::std::rc::Rc<::std::cell::RefCell<$crate::Manager>>,
            $(pub $emit: $crate::Signal<dyn $emit_type>),*
        }

        impl $hub {
            #[allow(dead_code)]
            pub fn subscribe<T>(&mut self, input: T::Input)
            where
                T: $crate::Nodified + $crate::Selfscriber<Self> + $crate::Subscriber,
                T::Node: for<'a> ::std::convert::From<&'a Self>,
            {
                self._private_revent_1_manager.borrow_mut().prepare_construction(T::name());

                let sub: T::Node = ::std::convert::From::from(&*self);

                let item = ::std::rc::Rc::new(::std::cell::RefCell::new(T::build(sub, input)));
                T::selfscribe(self, item);

                self._private_revent_1_manager.borrow_mut().finish_construction();
            }
        }

    };

    (from $hub:path, $source:path {
         $($emit:ident: $emit_type:path),*
     }) => {
        impl ::std::convert::From<&'_ $source> for $hub {
            fn from(item: &$source) -> Self {
                Self {
                    _private_revent_1_manager: item._private_revent_1_manager.clone(),
                    $($emit: item.$emit.internal_clone()),*
                }
            }
        }
    };

    (from $hub:path, $source:path, $($rest:path),+ {
         $($emit:ident: $emit_type:path),*
     }) => {
        crate::node_internal! {
            from $hub, $($rest),+ {
                $($emit: $emit_type),*
            }
        }
    };

    (selfscribe $on:path { $source:path } {
         $($listen:ident),*
     }) => {
        impl $crate::Selfscriber<$source> for $on {
            fn name() -> &'static str {
                stringify!($on)
            }
            fn selfscribe(holder: &$source, item: ::std::rc::Rc<::std::cell::RefCell<Self>>) {
                $(holder.$listen.insert(item.clone());)*
            }
        }

    };

    (selfscribe $on:path { $source:path, $($rest:path),+ } {
         $($listen:ident),*
     }) => {
        crate::node_internal! {
            selfscribe $on { $source } {
                $($listen),*
            }
        }
        crate::node_internal! {
            selfscribe $on { $($rest),+ } {
                $($listen),*
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn empty_hub_creation() {
        hub! {
            X {
            }
        }

        X::new();
        X::default();
    }

    #[test]
    fn hub_with_a_trait() {
        pub trait A {}
        hub! {
            X {
                a: A,
            }
        }

        X::new();
        X::default();
    }

    #[test]
    fn hub_with_node() {
        pub trait A {}

        hub! {
            X {
                a: A,
                b: A,
            }
        }

        node! {
            X {
                a: A,
            } => Node(Handler) {
                b: A,
            }
        }

        struct Handler;

        impl A for Handler {}

        X::new();
        X::default();
    }

    #[test]
    #[should_panic(expected = "[Handler]a -> a")]
    fn hub_recursion() {
        pub trait A {
            fn a(&mut self);
        }

        hub! {
            X {
                a: A,
            }
        }

        node! {
            X {
                a: A,
            } => Node(Handler) {
                a: A,
            }
        }

        struct Handler {
            node: Node,
        }

        impl A for Handler {
            fn a(&mut self) {
                self.node.a.emit(|x| {
                    x.a();
                });
            }
        }

        impl Subscriber for Handler {
            type Input = ();
            fn build(node: Self::Node, _: Self::Input) -> Self {
                Self { node }
            }
        }

        let mut x = X::new();
        x.subscribe::<Handler>(());

        x.a.emit(|x| x.a());
    }

    #[test]
    #[should_panic(expected = "[AToBHandler]a -> [BToAHandler]b -> a")]
    fn hub_dual_recursion() {
        pub trait A {
            fn a(&mut self);
        }

        pub trait B {
            fn b(&mut self);
        }

        hub! {
            X {
                a: A,
                b: B,
            }
        }

        node! {
            X {
                a: A,
            } => AToB(AToBHandler) {
                b: B,
            }
        }

        struct AToBHandler {
            node: AToB,
        }

        impl A for AToBHandler {
            fn a(&mut self) {
                self.node.b.emit(|x| {
                    x.b();
                });
            }
        }

        impl Subscriber for AToBHandler {
            type Input = ();
            fn build(node: Self::Node, _: Self::Input) -> Self {
                Self { node }
            }
        }

        node! {
            X {
                b: B,
            } => BToA(BToAHandler) {
                a: A,
            }
        }

        struct BToAHandler {
            node: BToA,
        }

        impl B for BToAHandler {
            fn b(&mut self) {
                self.node.a.emit(|x| {
                    x.a();
                });
            }
        }

        impl Subscriber for BToAHandler {
            type Input = ();
            fn build(node: Self::Node, _: Self::Input) -> Self {
                Self { node }
            }
        }

        let mut x = X::new();
        x.subscribe::<AToBHandler>(());
        x.subscribe::<BToAHandler>(());
    }

    #[test]
    fn sorting() {
        pub trait A {
            fn value(&self) -> i32;
        }

        hub! {
            X {
                a: A,
            }
        }

        node! {
            X {
                a: A,
            } => Node(Handler) {
            }
        }

        struct Handler(i32);

        impl A for Handler {
            fn value(&self) -> i32 {
                self.0
            }
        }
        impl Subscriber for Handler {
            type Input = i32;
            fn build(_: Self::Node, input: Self::Input) -> Self {
                Self(input)
            }
        }

        let mut x = X::new();

        for value in 0..10 {
            x.subscribe::<Handler>(value);
        }

        x.a.sort_by(|a, b| b.value().cmp(&a.value()));

        let mut count = 9;
        x.a.emit(|item| {
            assert_eq!(item.value(), count);
            count -= 1;
        });
    }
}
