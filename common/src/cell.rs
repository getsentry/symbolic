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

    #[test]
    fn test_try_new() {
        let result = SelfCell::try_new(String::from("42"), |s| Foo::parse(unsafe { &*s }));
        result.expect("parsing should not fail");

        let result = SelfCell::try_new(String::from("hello world"), |s| Foo::parse(unsafe { &*s }));
        result.expect_err("parsing should fail");
    }
}
