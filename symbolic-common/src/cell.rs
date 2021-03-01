//! Primitives for dealing with self-referential data.
//!
//! The types and traits in this module aim to work around the lack of self-referencial types in
//! Rust. This can happen when a _dependent_ type -- one that needs to borrow data without holding
//! on to the owning reference -- needs to be stored alongside its owner. This is inherently not
//! possible in Rust, since it would require the owner to have a stable memory address, but it is
//! moved along with the reference.
//!
//! This module solves this by introducing the `AsSelf` trait, which can be used to coerce the
//! lifetime of a dependent object to the lifetime of its owners at the time of the borrow.
//!
//! See [`SelfCell`] and [`AsSelf`] for more information.
//!
//! [`SelfCell`]: struct.SelfCell.html
//! [`AsSelf`]: trait.AsSelf.html

use std::ops::Deref;

pub use stable_deref_trait::StableDeref;

/// Safe downcasting of dependent lifetime bounds on structs.
///
/// This trait is similar to `AsRef`, except that it allows to capture the lifetime of the own
/// instance at the time of the borrow. This allows to force it onto the type's lifetime bounds.
/// This is particularly useful when the type's lifetime is somehow tied to it's own existence, such
/// as in self-referential structs. See [`SelfCell`] for an implementation that makes use of this.
///
/// # Implementation
///
/// While this trait may be implemented for any type, it is only useful for types that specify a
/// lifetime bound, such as `Cow` or [`ByteView`]. To implement, define `Ref` as the type with all
/// dependent lifetimes set to `'slf`. Then, simply return `self` in `as_self`.
///
/// ```rust
/// use symbolic_common::AsSelf;
///
/// struct Foo<'a>(&'a str);
///
/// impl<'slf> AsSelf<'slf> for Foo<'_> {
///     type Ref = Foo<'slf>;
///
///     fn as_self(&'slf self) -> &Self::Ref {
///         self
///     }
/// }
/// ```
///
/// # Interior Mutability
///
/// **Note** that if your type uses interior mutability (essentially any type from `std::sync`, but
/// specifically everything built on top of `UnsafeCell`), this implicit coercion will not work. The
/// compiler imposes this limitation by declaring any lifetime on such types as invariant, to avoid
/// interior mutations to write back data with the lowered lifetime.
///
/// If you are sure that your type will not borrow and store data of the lower lifetime, then
/// implement the coercion with an unsafe transmute:
///
/// ```rust
/// use std::cell::UnsafeCell;
/// use symbolic_common::AsSelf;
///
/// struct Foo<'a>(UnsafeCell<&'a str>);
///
/// impl<'slf> AsSelf<'slf> for Foo<'_> {
///     type Ref = Foo<'slf>;
///
///     fn as_self(&'slf self) -> &Self::Ref {
///         unsafe { std::mem::transmute(self) }
///     }
/// }
/// ```
///
/// [`SelfCell`]: struct.SelfCell.html
/// [`ByteView`]: struct.ByteView.html
pub trait AsSelf<'slf> {
    /// The `Self` type with `'slf` lifetimes, returned by `as_self`.
    type Ref: ?Sized;

    /// Returns a reference to `self` with downcasted lifetime.
    fn as_self(&'slf self) -> &Self::Ref;
}

impl AsSelf<'_> for u8 {
    type Ref = u8;

    fn as_self(&self) -> &Self::Ref {
        self
    }
}

impl AsSelf<'_> for str {
    type Ref = str;

    fn as_self(&self) -> &Self::Ref {
        self
    }
}

impl<'slf, T> AsSelf<'slf> for [T]
where
    T: AsSelf<'slf>,
    T::Ref: Sized,
{
    type Ref = [T::Ref];

    fn as_self(&'slf self) -> &Self::Ref {
        unsafe { &*(self as *const [T] as *const [T::Ref]) }
    }
}

impl<'slf, T> AsSelf<'slf> for &'slf T
where
    T: AsSelf<'slf> + ?Sized,
{
    type Ref = T::Ref;

    fn as_self(&'slf self) -> &Self::Ref {
        (*self).as_self()
    }
}

impl<'slf, T> AsSelf<'slf> for &'slf mut T
where
    T: AsSelf<'slf> + ?Sized,
{
    type Ref = T::Ref;

    fn as_self(&'slf self) -> &Self::Ref {
        (**self).as_self()
    }
}

impl<'slf, T> AsSelf<'slf> for Vec<T>
where
    T: AsSelf<'slf>,
    T::Ref: Sized,
{
    type Ref = [T::Ref];

    fn as_self(&'slf self) -> &Self::Ref {
        (**self).as_self()
    }
}

impl<'slf, T> AsSelf<'slf> for std::rc::Rc<T>
where
    T: AsSelf<'slf>,
{
    type Ref = T::Ref;

    fn as_self(&'slf self) -> &Self::Ref {
        (**self).as_self()
    }
}

impl<'slf, T> AsSelf<'slf> for std::sync::Arc<T>
where
    T: AsSelf<'slf>,
{
    type Ref = T::Ref;

    fn as_self(&'slf self) -> &Self::Ref {
        (**self).as_self()
    }
}

/// A container carrying a derived object alongside its owner.
///
/// **Warning**: This is an inherently unsafe type that builds on top of [`StableDeref`] and
/// [`AsSelf`] to establish somewhat safe memory semantics. Always try to avoid self-references by
/// storing data in an outer scope or avoiding the need alltogether, first.
///
/// `SelfCell` stores an owner object that must implement [`StableDeref`]. This guarantees that the
/// reference pointed to by the dependent object never moves over the lifetime of this object. This
/// is already implemented for most heap-allocating types, like `Box`, `Rc`, `Arc` or `ByteView`.
///
/// Additionally, the dependent object must implement [`AsSelf`]. This guarantees that the borrow's
/// lifetime and its lifetime bounds never exceed the lifetime of the owner. As such, an object
/// `Foo<'a>` that borrows data from the owner, will be coerced down to `Foo<'self>` when borrowing.
/// There are two constructor functions, `new` and `try_new`, each of which are passed a pointer to
/// the owned data. Dereferencing this pointer is intentionally unsafe, and beware that a borrow of
/// that pointer **must not** leave the callback.
///
/// While it is possible to store derived *references* in a `SelfCell`, too, there are simpler
/// alternatives, such as `owning_ref::OwningRef`. Consider using such types before using
/// `SelfCell`.
///
/// ## Example
///
/// ```rust
/// use symbolic_common::{AsSelf, SelfCell};
///
/// struct Foo<'a>(&'a str);
///
/// impl<'slf> AsSelf<'slf> for Foo<'_> {
///     type Ref = Foo<'slf>;
///
///     fn as_self(&'slf self) -> &Self::Ref {
///         self
///     }
/// }
///
/// let owner = String::from("hello world");
/// let cell = SelfCell::new(owner, |s| Foo(unsafe { &*s }));
/// assert_eq!(cell.get().0, "hello world");
/// ```
///
/// [`StableDeref`]: trait.StableDeref.html
/// [`AsSelf`]: trait.AsSelf.html
#[derive(Clone, Debug)]
pub struct SelfCell<O, D>
where
    O: StableDeref,
{
    owner: O,
    derived: D,
}

impl<'slf, O, T> SelfCell<O, T>
where
    O: StableDeref + 'slf,
    T: AsSelf<'slf>,
{
    /// Creates a new `SelfCell`.
    ///
    /// # Safety
    ///
    /// The callback receives a pointer to the owned data. Dereferencing the pointer is unsafe. Note
    /// that a borrow to that data can only safely be used to derive the object and **must not**
    /// leave the callback.
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::SelfCell;
    ///
    /// let owner = String::from("hello world");
    /// let cell = SelfCell::new(owner, |s| unsafe { &*s });
    /// ```
    #[inline]
    pub fn new<F>(owner: O, derive: F) -> Self
    where
        F: FnOnce(*const <O as Deref>::Target) -> T,
    {
        let derived = derive(owner.deref() as *const _);
        SelfCell { owner, derived }
    }

    /// Creates a new `SelfCell` which may fail to construct.
    ///
    /// # Safety
    ///
    /// The callback receives a pointer to the owned data. Dereferencing the pointer is unsafe. Note
    /// that a borrow to that data can only safely be used to derive the object and **must not**
    /// leave the callback.
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::SelfCell;
    ///
    /// fn main() -> Result<(), std::str::Utf8Error> {
    ///     let owner = Vec::from("hello world");
    ///     let cell = SelfCell::try_new(owner, |s| unsafe { std::str::from_utf8(&*s) })?;
    ///     Ok(())
    /// }
    /// ```
    #[inline]
    pub fn try_new<E, F>(owner: O, derive: F) -> Result<Self, E>
    where
        F: FnOnce(*const <O as Deref>::Target) -> Result<T, E>,
    {
        let derived = derive(owner.deref() as *const _)?;
        Ok(SelfCell { owner, derived })
    }

    /// Unsafely creates a new `SelfCell` from a derived object by moving the owner.
    ///
    /// # Safety
    ///
    /// This is an inherently unsafe process. The caller must guarantee that the derived object only
    /// borrows from the owner that is moved into this container and the borrowed reference has a
    /// stable address. This is useful, when cloning the owner by deriving a sub-object.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::sync::Arc;
    /// use symbolic_common::{AsSelf, SelfCell};
    ///
    /// struct Foo<'a>(&'a str);
    ///
    /// impl<'slf> AsSelf<'slf> for Foo<'_> {
    ///     type Ref = Foo<'slf>;
    ///
    ///     fn as_self(&'slf self) -> &Self::Ref {
    ///         self
    ///     }
    /// }
    ///
    /// // Create a clonable owner and move it into cell
    /// let owner = Arc::<str>::from("  hello  ");
    /// let cell = SelfCell::new(owner, |s| Foo(unsafe { &*s }));
    ///
    /// // Create a second derived object and clone the owner
    /// let trimmed = Foo(cell.get().0.trim());
    /// let cell2 = unsafe { SelfCell::from_raw(cell.owner().clone(), trimmed) };
    ///
    /// // Now, drop the original cell and continue using the clone
    /// assert_eq!(cell2.get().0, "hello");
    /// ```
    #[inline]
    pub unsafe fn from_raw(owner: O, derived: T) -> Self {
        SelfCell { owner, derived }
    }

    /// Returns a reference to the owner of this cell.
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::SelfCell;
    ///
    /// let owner = String::from("  hello  ");
    /// let cell = SelfCell::new(owner, |s| unsafe { (*s).trim() });
    /// assert_eq!(cell.owner(), "  hello  ");
    /// ```
    #[inline(always)]
    pub fn owner(&self) -> &O {
        &self.owner
    }

    /// Returns a safe reference to the derived object in this cell.
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::SelfCell;
    ///
    /// let owner = String::from("  hello  ");
    /// let cell = SelfCell::new(owner, |s| unsafe { (*s).trim() });
    /// assert_eq!(cell.get(), "hello");
    /// ```
    #[inline(always)]
    pub fn get(&'slf self) -> &'slf <T as AsSelf<'slf>>::Ref {
        self.derived.as_self()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use similar_asserts::assert_eq;

    #[derive(Debug, PartialEq)]
    struct Foo<'a>(&'a str);

    impl<'a> Foo<'a> {
        fn parse(s: &'a str) -> Result<Self, std::num::ParseIntError> {
            s.parse::<usize>()?;
            Ok(Foo(s))
        }
    }

    impl<'slf> AsSelf<'slf> for Foo<'_> {
        type Ref = Foo<'slf>;

        fn as_self(&'slf self) -> &Self::Ref {
            self
        }
    }

    #[test]
    fn test_new() {
        let fooref = SelfCell::new(String::from("hello world"), |s| Foo(unsafe { &*s }));
        assert_eq!(fooref.get().0, "hello world");
    }

    #[test]
    fn test_try_new() {
        let result = SelfCell::try_new(String::from("42"), |s| Foo::parse(unsafe { &*s }));
        result.expect("parsing should not fail");

        let result = SelfCell::try_new(String::from("hello world"), |s| Foo::parse(unsafe { &*s }));
        result.expect_err("parsing should fail");
    }
}
