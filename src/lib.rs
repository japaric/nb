//! Minimal and reusable non-blocking I/O layer
//!
//! The ultimate goal of this crate is *code reuse*. With this crate you can
//! write *core* I/O APIs that can then be adapted to operate in either blocking
//! or non-blocking manner. Furthermore those APIs are not tied to a particular
//! asynchronous model and can be adapted to work with the `futures` model or
//! with the `async` / `await` model.
//!
//! # Core idea
//!
//! The [`WouldBlock`](enum.Error.html) error variant signals that the operation
//! can't be completed *right now* and would need to block to complete.
//! [`WouldBlock`](enum.Error.html) is a special error in the sense that's not
//! *fatal*; the operation can still be completed by retrying again later.
//!
//! [`nb::Result`](type.Result.html) is based on the API of
//! [`std::io::Result`](https://doc.rust-lang.org/std/io/type.Result.html),
//! which has a `WouldBlock` variant in its
//! [`ErrorKind`](https://doc.rust-lang.org/std/io/enum.ErrorKind.html).
//!
//! We can map [`WouldBlock`](enum.Error.html) to different blocking and
//! non-blocking models:
//!
//! - In blocking mode: [`WouldBlock`](enum.Error.html) means try again right
//!   now (i.e. busy wait)
//! - In `futures` mode: [`WouldBlock`](enum.Error.html) means
//!   [`Async::NotReady`](https://docs.rs/futures)
//! - In `await` mode: [`WouldBlock`](enum.Error.html) means `yield`
//!   (suspend the generator)
//!
//! # How to use this crate
//!
//! Application specific errors can be put inside the `Other` variant in the
//! [`nb::Error`](enum.Error.html) enum.
//!
//! So in your API instead of returning `Result<T, MyError>` return
//! `nb::Result<T, MyError>`
//!
//! ```
//! enum MyError {
//!     ThisError,
//!     ThatError,
//!     // ..
//! }
//!
//! // This is a blocking function, so it returns a normal `Result`
//! fn before() -> Result<(), MyError> {
//!     // ..
//! #   Ok(())
//! }
//!
//! // This is now a potentially (read: *non*) blocking function so it returns `nb::Result`
//! // instead of blocking
//! fn after() -> nb::Result<(), MyError> {
//!     // ..
//! #   Ok(())
//! }
//! ```
//!
//! You can use the *never type* (`!`) to signal that some API has no fatal
//! errors but may block:
//!
//! ```
//! #![feature(never_type)]
//!
//! // This returns `Ok(())` or `Err(nb::Error::WouldBlock)`
//! fn maybe_blocking_api() -> nb::Result<(), !> {
//!     // ..
//! #   Ok(())
//! }
//! ```
//!
//! Once your API uses [`nb::Result`](type.Result.html) you can leverage the
//! [`block!`], [`try_nb!`] and [`await!`] macros to adapt it for blocking
//! operation, or for non-blocking operation with `futures` or `await`.
//!
//! **NOTE** Currently, both `try_nb!` and `await!` are feature gated behind the `unstable` Cargo
//! feature.
//!
//! [`block!`]: macro.block.html
//! [`try_nb!`]: macro.try_nb.html
//! [`await!`]: macro.await.html
//!
//! # Examples
//!
//! ## A Core I/O API
//!
//! Imagine the code (crate) below represents a Hardware Abstraction Layer for some microcontroller
//! (or microcontroller family).
//!
//! *In this and the following examples let's assume for simplicity that peripherals are treated
//! as global singletons and that no preemption is possible (i.e. interrupts are disabled).*
//!
//! ```
//! #![feature(never_type)]
//!
//! // This is the `hal` crate
//! // Note that it doesn't depend on the `futures` crate
//!
//! extern crate nb;
//!
//! /// An LED
//! pub struct Led;
//!
//! impl Led {
//!     pub fn off(&self) {
//!         // ..
//!     }
//!     pub fn on(&self) {
//!         // ..
//!     }
//! }
//!
//! /// Serial interface
//! pub struct Serial;
//! pub enum Error {
//!     Overrun,
//!     // ..
//! }
//!
//! impl Serial {
//!     /// Reads a single byte from the serial interface
//!     pub fn read(&self) -> nb::Result<u8, Error> {
//!         // ..
//! #       Ok(0)
//!     }
//!
//!     /// Writes a single byte to the serial interface
//!     pub fn write(&self, byte: u8) -> nb::Result<(), Error> {
//!         // ..
//! #       Ok(())
//!     }
//! }
//!
//! /// A timer used for timeouts
//! pub struct Timer;
//!
//! impl Timer {
//!     /// Waits until the timer times out
//!     pub fn wait(&self) -> nb::Result<(), !> {
//!         //^ NOTE the `!` indicates that this operation can block but has no
//!         //  other form of error
//!
//!         // ..
//! #       Ok(())
//!     }
//! }
//! ```
//!
//! ## Blocking mode
//!
//! Turn on an LED for one second and *then* loops back serial data.
//!
//! ```
//! # #![feature(never_type)]
//! #[macro_use(block)]
//! extern crate nb;
//!
//! use hal::{Led, Serial, Timer};
//!
//! fn main() {
//!     // Turn the LED on for one second
//!     Led.on();
//!     block!(Timer.wait()).unwrap(); // NOTE(unwrap) E = !
//!     Led.off();
//!
//!     // Serial interface loopback
//!     # return;
//!     loop {
//!         let byte = block!(Serial.read()).unwrap();
//!         block!(Serial.write(byte)).unwrap();
//!     }
//! }
//!
//! # mod hal {
//! #   use nb;
//! #   pub struct Led;
//! #   impl Led {
//! #       pub fn off(&self) {}
//! #       pub fn on(&self) {}
//! #   }
//! #   pub struct Serial;
//! #   impl Serial {
//! #       pub fn read(&self) -> nb::Result<u8, ()> { Ok(0) }
//! #       pub fn write(&self, _: u8) -> nb::Result<(), ()> { Ok(()) }
//! #   }
//! #   pub struct Timer;
//! #   impl Timer {
//! #       pub fn wait(&self) -> nb::Result<(), !> { Ok(()) }
//! #   }
//! # }
//! ```
//!
//! ## `futures`
//!
//! Blinks an LED every second *and* loops back serial data. Both tasks run
//! concurrently.
//!
//! ```
//! #![feature(conservative_impl_trait)]
//! #![feature(never_type)]
//!
//! extern crate futures;
//! #[macro_use(try_nb)]
//! extern crate nb;
//!
//! use futures::{Async, Future};
//! use futures::future::{self, Loop};
//! use hal::{Error, Led, Serial, Timer};
//!
//! /// `futures` version of `Timer.wait`
//! ///
//! /// This returns a future that must be polled to completion
//! fn wait() -> impl Future<Item = (), Error = !> {
//!     future::poll_fn(|| {
//!         Ok(Async::Ready(try_nb!(Timer.wait())))
//!     })
//! }
//!
//! /// `futures` version of `Serial.read`
//! ///
//! /// This returns a future that must be polled to completion
//! fn read() -> impl Future<Item = u8, Error = Error> {
//!     future::poll_fn(|| {
//!         Ok(Async::Ready(try_nb!(Serial.read())))
//!     })
//! }
//!
//! /// `futures` version of `Serial.write`
//! ///
//! /// This returns a future that must be polled to completion
//! fn write(byte: u8) -> impl Future<Item = (), Error = Error> {
//!     future::poll_fn(move || {
//!         Ok(Async::Ready(try_nb!(Serial.write(byte))))
//!     })
//! }
//!
//! fn main() {
//!     // Tasks
//!     let mut blinky = future::loop_fn::<_, (), _, _>(true, |state| {
//!         wait().map(move |_| {
//!             if state {
//!                 Led.on();
//!             } else {
//!                 Led.off();
//!             }
//!
//!             Loop::Continue(!state)
//!         })
//!     });
//!
//!     let mut loopback = future::loop_fn::<_, (), _, _>((), |_| {
//!         read().and_then(|byte| {
//!             write(byte)
//!         }).map(|_| {
//!             Loop::Continue(())
//!         })
//!     });
//!
//!     // Event loop
//!     loop {
//!         blinky.poll().unwrap(); // NOTE(unwrap) E = !
//!         loopback.poll().unwrap();
//!         # break
//!     }
//! }
//!
//! # mod hal {
//! #   use nb;
//! #   pub struct Led;
//! #   impl Led {
//! #       pub fn off(&self) {panic!()}
//! #       pub fn on(&self) {}
//! #   }
//! #   #[derive(Debug)]
//! #   pub enum Error {}
//! #   pub struct Serial;
//! #   impl Serial {
//! #       pub fn read(&self) -> nb::Result<u8, Error> { Err(nb::Error::WouldBlock) }
//! #       pub fn write(&self, _: u8) -> nb::Result<(), Error> { Err(nb::Error::WouldBlock) }
//! #   }
//! #   pub struct Timer;
//! #   impl Timer {
//! #       pub fn wait(&self) -> nb::Result<(), !> { Err(nb::Error::WouldBlock) }
//! #   }
//! # }
//! ```
//!
//! ## `await!`
//!
//! This is equivalent to the `futures` example but with much less boilerplate.
//!
//! ```
//! #![feature(generator_trait)]
//! #![feature(generators)]
//! #![feature(never_type)]
//!
//! #[macro_use(await)]
//! extern crate nb;
//!
//! use std::ops::Generator;
//!
//! use hal::{Led, Serial, Timer};
//!
//! fn main() {
//!     // Tasks
//!     let mut blinky = || {
//!         let mut state = false;
//!         loop {
//!             // `await!` means suspend / yield instead of blocking
//!             await!(Timer.wait()).unwrap(); // NOTE(unwrap) E = !
//!
//!             state = !state;
//!
//!             if state {
//!                  Led.on();
//!             } else {
//!                  Led.off();
//!             }
//!         }
//!     };
//!
//!     let mut loopback = || {
//!         loop {
//!             let byte = await!(Serial.read()).unwrap();
//!             await!(Serial.write(byte)).unwrap();
//!         }
//!     };
//!
//!     // Event loop
//!     loop {
//!         blinky.resume();
//!         loopback.resume();
//!         # break
//!     }
//! }
//!
//! # mod hal {
//! #   use nb;
//! #   pub struct Led;
//! #   impl Led {
//! #       pub fn off(&self) {}
//! #       pub fn on(&self) {}
//! #   }
//! #   pub struct Serial;
//! #   impl Serial {
//! #       pub fn read(&self) -> nb::Result<u8, ()> { Err(nb::Error::WouldBlock) }
//! #       pub fn write(&self, _: u8) -> nb::Result<(), ()> { Err(nb::Error::WouldBlock) }
//! #   }
//! #   pub struct Timer;
//! #   impl Timer {
//! #       pub fn wait(&self) -> nb::Result<(), !> { Err(nb::Error::WouldBlock) }
//! #   }
//! # }
//! ```

#![no_std]
#![deny(warnings)]

use core::fmt;

/// A non-blocking result
pub type Result<T, E> = ::core::result::Result<T, Error<E>>;

/// A non-blocking error
///
/// The main use of this enum is to add a `WouldBlock` variant to an existing
/// error enum.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Error<E> {
    /// A different kind of error
    Other(E),
    /// This operation requires blocking behavior to complete
    WouldBlock,
}

impl<E> fmt::Debug for Error<E>
where
    E: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Other(ref e) => fmt::Debug::fmt(e, f),
            Error::WouldBlock => f.write_str("WouldBlock"),
        }
    }
}

impl<E> Error<E> {
    /// Maps an `Error<E>` to `Error<T>` by applying a function to a contained
    /// `Error::Other` value, leaving an `Error::WouldBlock` value untouched.
    pub fn map<T, F>(self, op: F) -> Error<T> where F: FnOnce(E) -> T {
        match self {
            Error::Other(e) => Error::Other(op(e)),
            Error::WouldBlock => Error::WouldBlock,
        }
    }
}

impl<E> From<E> for Error<E> {
    fn from(error: E) -> Error<E> {
        Error::Other(error)
    }
}

/// Await operation (*won't work until the language gains support for
/// generators*)
///
/// This macro evaluates the expression `$e` *cooperatively* yielding control
/// back to the (generator) caller whenever `$e` evaluates to
/// `Error::WouldBlock`.
///
/// # Requirements
///
/// This macro must be called within a generator body.
///
/// # Input
///
/// An expression `$e` that evaluates to `nb::Result<T, E>`
///
/// # Output
///
/// - `Ok(t)` if `$e` evaluates to `Ok(t)`
/// - `Err(e)` if `$e` evaluates to `Err(nb::Error::Other(e))`
#[cfg(feature = "unstable")]
#[macro_export]
macro_rules! await {
    ($e:expr) => {
        loop {
            #[allow(unreachable_patterns)]
            match $e {
                Err($crate::Error::Other(e)) => {
                    #[allow(unreachable_code)]
                    break Err(e)
                },
                Err($crate::Error::WouldBlock) => {}, // yield (see below)
                Ok(x) => break Ok(x),
            }

            yield
        }
    }
}

/// Turns the non-blocking expression `$e` into a blocking operation.
///
/// This is accomplished by continuously calling the expression `$e` until it no
/// longer returns `Error::WouldBlock`
///
/// # Input
///
/// An expression `$e` that evaluates to `nb::Result<T, E>`
///
/// # Output
///
/// - `Ok(t)` if `$e` evaluates to `Ok(t)`
/// - `Err(e)` if `$e` evaluates to `Err(nb::Error::Other(e))`
#[macro_export]
macro_rules! block {
    ($e:expr) => {
        loop {
            #[allow(unreachable_patterns)]
            match $e {
                Err($crate::Error::Other(e)) => {
                    #[allow(unreachable_code)]
                    break Err(e)
                },
                Err($crate::Error::WouldBlock) => {},
                Ok(x) => break Ok(x),
            }
        }
    }
}

/// Turns the non-blocking expression `$e` into a blocking operation for as long
/// as the given expression evaluates to true.
///
/// This is accomplished by continuously calling the expression `$e` until it no
/// longer returns `Error::WouldBlock` and by calling expression `$c` to evaluate
/// whether to keep polling. If `$c` evaluates to false and `$e` evaluates to
/// `Error::WouldBlock`, `Err(nb::Error::WouldBlock)` is returned.
///
/// # Input
///
/// An expression `$c` that evaluates to `bool`
/// An expression `$e` that evaluates to `nb::Result<T, E>`
///
/// # Output
///
/// - `Ok(t)` if `$e` evaluates to `Ok(t)`
/// - `Err(nb::Error::Other(e))` if `$e` evaluates to `Err(nb::Error::Other(e))`
/// - `Err(Error::WouldBlock)` if `$e` evaluates to `Err(Error::WouldBlock)` and `$c` evaluates to false
#[macro_export]
macro_rules! block_while {
    ($c:expr, $e:expr) => {
        loop {
            #[allow(unreachable_patterns)]
            match $e {
                Err($crate::Error::Other(e)) => {
                    #[allow(unreachable_code)]
                    break Err($crate::Error::Other(e))
                },
                Err($crate::Error::WouldBlock) => {
                    if !$c {
                        break Err($crate::Error::WouldBlock);
                    }
                },
                Ok(x) => break Ok(x),
            }
        }
    }
}

/// Future adapter
///
/// This is a *try* operation from a `nb::Result` to a `futures::Poll`
///
/// # Requirements
///
/// This macro must be called within a function / closure that has signature
/// `fn(..) -> futures::Poll<T, E>`.
///
/// This macro requires that the [`futures`] crate is in the root of the crate.
///
/// [`futures`]: https://crates.io/crates/futures
///
/// # Input
///
/// An expression `$e` that evaluates to `nb::Result<T, E>`
///
/// # Early return
///
/// - `Ok(Async::NotReady)` if `$e` evaluates to `Err(nb::Error::WouldBlock)`
/// - `Err(e)` if `$e` evaluates to `Err(nb::Error::Other(e))`
///
/// # Output
///
/// `t` if `$e` evaluates to `Ok(t)`
#[cfg(feature = "unstable")]
#[macro_export]
macro_rules! try_nb {
    ($e:expr) => {
        match $e {
            Err($crate::Error::Other(e)) => return Err(e),
            Err($crate::Error::WouldBlock) => {
                return Ok(::futures::Async::NotReady)
            },
            Ok(x) => x,
        }
    }
}
