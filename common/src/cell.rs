use std::ops::Deref;

pub use stable_deref_trait::StableDeref;

pub trait AsSelf<'slf> {
    type Ref: ?Sized;

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
    T: AsSelf<'slf>,
{
    type Ref = T::Ref;

    fn as_self(&'slf self) -> &Self::Ref {
        (*self).as_self()
    }
}

impl<'slf, T> AsSelf<'slf> for &'slf mut T
where
    T: AsSelf<'slf>,
{
    type Ref = T::Ref;

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
    #[inline]
    pub fn new<F>(owner: O, derive: F) -> Self
    where
        F: Fn(*const <O as Deref>::Target) -> T,
    {
        let derived = derive(owner.deref() as *const _);
        SelfCell { owner, derived }
    }

    #[inline]
    pub fn try_new<E, F>(owner: O, derive: F) -> Result<Self, E>
    where
        F: Fn(*const <O as Deref>::Target) -> Result<T, E>,
    {
        let derived = derive(owner.deref() as *const _)?;
        Ok(SelfCell { owner, derived })
    }

    #[inline]
    pub unsafe fn from_raw(owner: O, derived: T) -> Self {
        SelfCell { owner, derived }
    }

    // #[inline]
    // pub fn map<U, F>(self, f: F) -> SelfCell<O, U>
    // where
    //     U: AsSelf<'slf>,
    //     F: Fn(T) -> U,
    // {
    //     SelfCell {
    //         owner: self.owner,
    //         derived: f(self.derived),
    //     }
    // }

    // #[inline]
    // pub fn try_map<U, E, F>(self, f: F) -> Result<SelfCell<O, U>, E>
    // where
    //     U: AsSelf<'slf>,
    //     F: Fn(T) -> Result<U, E>,
    // {
    //     Ok(SelfCell {
    //         owner: self.owner,
    //         derived: f(self.derived)?,
    //     })
    // }

    // #[inline]
    // #[allow(clippy::should_implement_trait)]
    // pub fn as_ref(&self) -> SelfCell<O, &T>
    // where
    //     O: Clone,
    // {
    //     SelfCell {
    //         owner: self.owner.clone(),
    //         derived: &self.derived,
    //     }
    // }

    // pub fn transform<F, U>(self, f: F) -> U
    // where
    //     F: FnOnce(&O, T) -> U,
    // {
    //     f(&self.owner, self.derived)
    // }

    // #[inline]
    // pub fn clone_with<'b, U, F>(&self, f: F) -> SelfCell<O, U>
    // where
    //     O: Clone,
    //     U: AsSelf<'b>,
    //     F: FnOnce(&T) -> U,
    // {
    //     self.as_ref().map(f)
    // }

    #[inline]
    pub fn owner(&self) -> &O {
        &self.owner
    }

    pub fn get(&'slf self) -> &<T as AsSelf>::Ref {
        self.derived.as_self()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // Should not compile due to lifetime conflict
    // fn test_invalid() {
    //     let outer = {
    //         let fooref = SelfCell::new(String::from("hello world"), |s| Foo(unsafe { &*s }));
    //         fooref.get().0
    //     };
    //     assert_eq!(outer, "hello world");
    // }

    #[test]
    fn test_try_new() {
        let result = SelfCell::try_new(String::from("42"), |s| Foo::parse(unsafe { &*s }));
        result.expect("parsing should not fail");

        let result = SelfCell::try_new(String::from("hello world"), |s| Foo::parse(unsafe { &*s }));
        result.expect_err("parsing should fail");
    }

    // #[test]
    // fn test_map() {
    //     let fooref = SelfCell::new(String::from("  hello  "), Foo);
    //     let mapped = fooref.map(|f| Foo(f.0.trim()));
    //     assert_eq!(mapped.get().0, "hello");
    // }

    // #[test]
    // fn test_map_invalid() {
    //     let mut outer = None;
    //     let fooref = SelfCell::new(String::from("  hello  "), |s| {
    //         outer = Some(s);
    //         Foo(s)
    //     });
    //     std::mem::drop(fooref)
    //     // TODO(ja): this is a use after free
    //     assert_eq!("  hello  ", outer.unwrap());
    // }

    // #[test]
    // fn test_try_map() {
    //     let fooref = SelfCell::new(String::from("  42  "), Foo);
    //     let result = fooref.try_map(|f| Foo::parse(f.0.trim()));
    //     result.expect("parsing should not fail");

    //     let fooref = SelfCell::new(String::from("  hello  "), Foo);
    //     let result = fooref.try_map(|f| Foo::parse(f.0.trim()));
    //     result.expect_err("parsing should fail");
    // }

    // #[test]
    // fn test_as_ref() {
    //     let fooref = SelfCell::new(String::from("hello world"), Foo);
    //     let foorefref = fooref.as_ref();
    //     assert_eq!(foorefref.get(), fooref.get());
    //     assert_eq!(
    //         foorefref.get() as *const Foo<'_>,
    //         fooref.get() as *const Foo<'_>
    //     );
    // }

    // #[test]
    // fn test_split() {
    //     let fooref = SelfCell::new(String::from("helloworld"), Foo);
    //     let (hello, world) = fooref.transmute(|string, foo| {
    //         let (hello, world) = fooref.get().split_at(5);
    //         (
    //             SelfCell::new(string.clone(), hello),
    //             SelfCell::new(string.clone(), world),
    //         )
    //     });
    // }
}
